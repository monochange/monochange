#!/usr/bin/env node
// leptoskit tooling plugin for the monochange web frontends.
//
// The upstream leptoskit build compiles the server, the wasm client,
// wasm-bindgen, and the split chunks, and release-builds precompress the
// output. What it does not build is the site CSS or a minified split loader;
// this plugin owns both inside `leptoskit build` (and `leptoskit dev`), on the
// documented `[[plugins]]` process protocol (protocol 1, see
// crates/leptoskit_plugin/src/process.rs upstream): one JSON envelope on
// stdin, one JSON reply on stdout, one process per hook.
//
// `after_build` — which upstream runs before its release precompression —
//
// 1. minifies the wasm-split loader with the guarded shared implementation
//    (scripts/minify-split-loader.ts);
// 2. compiles the shared Tailwind input to `pkg/<output_name>.css`;
// 3. places the frontend's pkg output under `<site_root>/pkg/<namespace>/`,
//    so the two web frontends assemble into one site root without name
//    collisions (each entry's split manifest, loader, and chunks live in
//    their own directory, and upstream's per-build `pkg/` wipe cannot touch
//    the other frontend's namespace);
// 4. precompresses the assembled site with gzip and brotli — upstream's
//    per-build precompression covers one output directory, while the
//    assembled tree spans both builds, so the plugin owns it (same
//    compressible set and quality: gzip level 9, brotli quality 11).
//
// `namespace`, `site_root`, and `tailwind_input` (relative to the project
// root, which is the CLI's working directory for every documented
// invocation) come from the `[[plugins]]` options in `app/leptoskit.toml`
// and `app/projects/leptoskit.toml`; both configs set
// `bundle.precompress = false` so compression happens exactly once, over the
// assembled tree. Missing tools or assets fail the hook loudly rather than
// producing an unstyled or oversized deployment.
//
// The plugin resolves as `leptoskit-plugin-monochange-web-assets` on PATH
// (devenv, CI, and Docker provide it from this directory), so the same
// declaration works from every project root.
//
// Logs go to stderr only; stdout carries the JSON reply and nothing else.
import { spawnSync } from "node:child_process";
import {
	cpSync,
	existsSync,
	mkdirSync,
	readFileSync,
	readSync,
	readdirSync,
	renameSync,
	rmSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { brotliCompressSync, constants as zlibConstants, gzipSync } from "node:zlib";

import { minifySplitLoaders } from "../minify-split-loader.ts";

const PROTOCOL = 1;
const PLUGIN_NAME = "monochange-web-assets";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..", "..");

type Envelope = {
	protocol?: number;
	plugin?: string;
	hook?: string;
	context?: PluginContext & CommandContext;
};

type PluginContext = {
	root?: string;
	platform?: string;
	release?: boolean;
	output_dir?: string;
	wasm_bundle?: string | null;
	options?: Record<string, unknown>;
};

type CommandContext = {
	command?: string;
	args?: string[];
};

class PluginFailure extends Error {}

function fail(message: string): never {
	throw new PluginFailure(message);
}

function log(message: string): void {
	console.error(`[${PLUGIN_NAME}] ${message}`);
}

function readStdin(): string {
	const chunks: Buffer[] = [];
	const buffer = Buffer.alloc(1 << 16);
	try {
		for (;;) {
			let read = -1;

			while (read === -1) {
				try {
					read = readSync(0, buffer);
				} catch (error) {
					const code = (error as NodeJS.ErrnoException).code;

					if (code === "EAGAIN" || code === "EINTR") continue;
					throw error;
				}
			}

			if (read === 0) break;
			chunks.push(buffer.subarray(0, read).subarray());
		}
	} catch (error) {
		fail(
			`could not read the hook envelope: ${error instanceof Error ? error.message : String(error)}`,
		);
	}

	return Buffer.concat(chunks).toString("utf8");
}

/** Resolves the Tailwind CSS compiler: override env, project and repo bins, then PATH. */
function resolveTailwind(root: string): string {
	const override = process.env.MONOCHANGE_TAILWIND_BIN;

	if (override !== undefined) {
		if (!existsSync(override)) {
			fail(`MONOCHANGE_TAILWIND_BIN does not exist: ${override}`);
		}

		return override;
	}

	// The app workspace owns the styles; its devDependency provides the CLI.
	const candidates = [
		resolve(root, "node_modules/.bin/tailwindcss"),
		// The leptoskit project root can sit below the repository (the
		// projects config lives in app/projects/); fall back to the repo.
		join(repoRoot, "app/node_modules/.bin/tailwindcss"),
	];

	for (const candidate of candidates) {
		if (existsSync(candidate)) return candidate;
	}

	return "tailwindcss";
}

function compileTailwind(input: string, output: string, tailwindBin: string): void {
	if (!existsSync(input)) {
		fail(`the Tailwind input file does not exist: ${input}`);
	}

	const result = spawnSync(
		tailwindBin,
		["-i", input, "-o", output, "--minify", "--log-level=error"],
		{ encoding: "utf8" },
	);

	if (result.error !== undefined) {
		fail(
			`could not run the Tailwind compiler (${tailwindBin}): ${result.error.message}; ` +
				"install the tailwindcss CLI or set MONOCHANGE_TAILWIND_BIN",
		);
	}

	if (result.status !== 0) {
		const detail = (result.stderr ?? result.stdout ?? `exit code ${result.status}`).slice(0, 4000);
		fail(`Tailwind failed for ${input}:\n${detail}`);
	}

	if (!existsSync(output) || statSync(output).size === 0) {
		fail(`Tailwind produced no CSS at ${output}`);
	}

	log(`compiled ${input} → ${output}`);
}

function afterBuild(context: PluginContext): void {
	const platform = context.platform ?? "web";

	if (platform !== "web") {
		log(`skipping ${platform}: only web builds carry browser assets`);

		return;
	}

	const outputDir =
		context.output_dir ?? fail("the after_build envelope is missing the output directory");
	const wasmBundle =
		context.wasm_bundle ?? fail("the after_build envelope is missing the wasm bundle");

	const options = context.options ?? {};

	// The after_build envelope carries no project root (upstream provides it
	// only on generate, a separate invocation), and every hook is one process:
	// resolve project-relative paths against the CLI's working directory,
	// which the documented invocations (devenv, CI, Docker) keep at the
	// project root.
	const root = process.cwd();

	const namespace = options.namespace;
	if (typeof namespace !== "string" || namespace.length === 0 || /[/\\]/.test(namespace)) {
		fail("plugin options must declare a `namespace` without path separators");
	}

	const tailwindInput = options.tailwind_input;
	if (typeof tailwindInput !== "string" || tailwindInput.length === 0) {
		fail("plugin options must declare a `tailwind_input` relative to the project root");
	}

	const siteRootOption = options.site_root;
	if (typeof siteRootOption !== "string" || siteRootOption.length === 0) {
		fail("plugin options must declare a `site_root` relative to the project root");
	}

	// The wasm-bindgen output names the entry: `{output_name}.js` and
	// `{output_name}_bg.wasm`, plus the split loader and manifest beside them.
	const wasmName = wasmBundle.split("/").at(-1) ?? "";
	if (!wasmName.endsWith("_bg.wasm")) {
		fail(`unexpected wasm bundle name ${wasmName}; expected {output_name}_bg.wasm`);
	}

	const outputName = wasmName.slice(0, -"_bg.wasm".length);

	const pkgDir = join(resolve(outputDir), "pkg");
	if (!existsSync(pkgDir)) {
		fail(`the build produced no ${pkgDir}; the web build must run before this hook`);
	}
	if (!existsSync(wasmBundle)) {
		fail(`the built wasm module is missing: ${wasmBundle}`);
	}

	// 1. Minify the split loader with the shared guarded implementation.
	const summary = minifySplitLoaders(pkgDir);
	log(`minified ${summary.loaders.length} split loader(s) with ${summary.esbuildBin}`);

	// 2. Compile the site CSS into the bundle directory.
	const cssPath = join(pkgDir, `${outputName}.css`);
	compileTailwind(resolve(root, tailwindInput), cssPath, resolveTailwind(root));

	// 3. Assemble: this frontend's bundle moves under the shared site root's
	// pkg/<namespace>/, keeping relative loader and chunk URLs inside the
	// entry's own directory. Upstream wipes its per-build pkg/ when a build's
	// fingerprint changes, so each build owns a scratch out directory that
	// upstream may wipe, and the plugin owns the move into the shared tree.
	// The build's root files (public assets, favicons) move to the site root
	// the same way; the website build contributes them, the projects build
	// contributes only its pkg.
	const siteRoot = resolve(root, siteRootOption);
	mkdirSync(siteRoot, { recursive: true });
	const outDirResolved = resolve(outputDir);
	moveInto(pkgDir, join(siteRoot, "pkg", namespace));
	for (const entry of readdirSync(outDirResolved)) {
		if (entry === "pkg") continue;
		const from = join(outDirResolved, entry);
		const to = join(siteRoot, entry);
		if (statSync(from).isDirectory()) {
			moveInto(from, to);
		} else {
			moveFile(from, to);
		}
	}

	log(`assembled the ${outputName} bundle into ${siteRoot}/pkg/${namespace}/`);

	// 4. Precompress the assembled site (bundle.precompress is false in both
	// web configs, so this is the only compression pass).
	const compressed = precompressSite(siteRoot);
	log(`precompressed the assembled site (${compressed.gzip} gzip, ${compressed.brotli} brotli)`);
}

/**
 * Moves one file to `target`, replacing it when present. Rename is atomic on
 * one filesystem and falls back to a copy for cross-device targets (a build
 * cache mount is a different filesystem from the output layer).
 */
function moveFile(from: string, to: string): void {
	try {
		renameSync(from, to);
	} catch (error) {
		const code = (error as NodeJS.ErrnoException).code;
		if (code !== "EXDEV") throw error;
		cpSync(from, to);
		rmSync(from);
	}
}

/**
 * Moves a directory's contents into `target`, replacing existing files so a
 * rebuild refreshes the assembled tree. Directory shells may remain in the
 * scratch out directory; only the site root's contents matter.
 */
function moveInto(source: string, target: string): void {
	mkdirSync(target, { recursive: true });
	for (const entry of readdirSync(source)) {
		const from = join(source, entry);
		const to = join(target, entry);
		if (statSync(from).isDirectory()) {
			moveInto(from, to);
		} else {
			moveFile(from, to);
		}
	}
}

// The compressible set upstream's compress.rs precompresses.
const COMPRESSIBLE = /\.(?:wasm|js|mjs|css|html|svg|json|map|txt|xml|webmanifest)$/u;
// Upstream's twin names: `<file>.gz` and `<file>.br` beside the source.
const PRECOMPRESSED = /\.(?:gz|br)$/u;

function precompressSite(siteRoot: string): { gzip: number; brotli: number } {
	let gzipCount = 0;
	let brotliCount = 0;
	const stack = [siteRoot];
	while (stack.length > 0) {
		const directory = stack.pop() ?? fail("unreachable directory entry");
		for (const entry of readdirSync(directory, { withFileTypes: true })) {
			const path = join(directory, entry.name);
			if (entry.isDirectory()) {
				stack.push(path);
				continue;
			}
			if (!entry.isFile() || PRECOMPRESSED.test(path) || !COMPRESSIBLE.test(path)) continue;
			const raw = readFileSync(path);
			writeFileSync(`${path}.gz`, gzipSync(raw, { level: 9 }));
			writeFileSync(
				`${path}.br`,
				brotliCompressSync(raw, {
					params: {
						[zlibConstants.BROTLI_PARAM_QUALITY]: 11,
						[zlibConstants.BROTLI_PARAM_LGWIN]: 24,
					},
				}),
			);
			gzipCount += 1;
			brotliCount += 1;
		}
	}
	return { gzip: gzipCount, brotli: brotliCount };
}

function reply(payload: Record<string, unknown>): never {
	process.stdout.write(`${JSON.stringify({ ok: true, protocol: PROTOCOL, ...payload })}\n`);
	process.exit(0);
}

function handle(envelope: Envelope): never {
	const hook = envelope.hook ?? fail("the envelope is missing the hook name");
	switch (hook) {
		case "describe": {
			reply({ platforms: [], commands: [] });
		}
		case "generate":
		case "before_build":
		case "before_serve": {
			reply({});
		}
		case "after_build": {
			afterBuild(envelope.context ?? {});
			reply({});
		}
		case "command": {
			fail("this plugin declares no commands");
		}
		default: {
			fail(`unknown hook ${hook}`);
		}
	}
}

function main(): never {
	const envelope = JSON.parse(readStdin()) as Envelope;
	if (envelope.protocol !== PROTOCOL) {
		fail(`unsupported protocol ${String(envelope.protocol)}; this plugin speaks ${PROTOCOL}`);
	}
	if (envelope.plugin !== undefined && envelope.plugin !== PLUGIN_NAME) {
		fail(`this plugin is ${PLUGIN_NAME}, not ${envelope.plugin}`);
	}

	handle(envelope);
}

try {
	main();
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	console.error(`[${PLUGIN_NAME}] ${message}`);
	process.exit(1);
}
