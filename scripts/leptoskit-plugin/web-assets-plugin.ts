#!/usr/bin/env node
// leptoskit tooling plugin for the monochange web frontends.
//
// The upstream leptoskit build compiles the server, the wasm client,
// wasm-bindgen, and the split chunks. What it does not build is the site CSS
// or a minified split loader; this plugin owns both inside `leptoskit build`
// (and `leptoskit dev`), on the documented `[[plugins]]` process protocol
// (protocol 1, see crates/leptoskit_plugin/src/process.rs upstream): one JSON
// envelope on stdin, one JSON reply on stdout, one process per hook.
//
// `after_build` — which upstream runs BEFORE its bundle report and budget
// check (crates/leptoskit_cli/src/build.rs at the pinned revision) — works
// strictly inside the build's own scratch output directory:
//
// 1. minifies the wasm-split loader and the entry glue with the guarded
//    shared implementation (scripts/minify-split-loader.ts);
// 2. compiles the shared Tailwind input to `pkg/monochange_app.css`;
// 3. subsets the source fonts to WOFF2 into `.font-subsets/`;
// 4. release builds re-optimize the WASM modules, preserving linking
//    contracts and rejecting increases in gzip or deployed brotli transfer.
//
// The plugin never moves or publishes anything: the SDK's bundle report and
// `bundle.budget_kib` check read the scratch output after this hook, and a
// build its own budget rejects must never have reached the deployed tree.
// Publication is a separate step (scripts/leptoskit-plugin/publish-web-site.ts)
// that runs only after BOTH web builds succeed; see build:web in devenv.nix.
//
// `tailwind_input` and `font_source` (relative to the project root, which is
// the CLI's working directory for every documented invocation) come from the
// `[[plugins]]` options in `app/leptoskit.toml` and `app/projects/leptoskit.toml`.
// Both frontends compile the same canonical input, so their CSS must come out
// byte-identical — the publisher verifies that before staging. Missing tools
// or assets fail the hook loudly rather than producing an unstyled or
// oversized deployment.
//
// The plugin resolves as `leptoskit-plugin-monochange-web-assets` on PATH
// (devenv, CI, and Docker provide it from this directory), so the same
// declaration works from every project root.
//
// Logs go to stderr only; stdout carries the JSON reply and nothing else.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { minifyEntryGlue, minifySplitLoaders } from "../minify-split-loader.ts";
import { optimizeWebWasm } from "./optimize-web-wasm.ts";

const PROTOCOL = 1;
const PLUGIN_NAME = "monochange-web-assets";
const FONT_NAMES = ["bricolage-800", "hanken-400", "hanken-700"];
const FONT_UNICODES =
	"U+0000-024F,U+0300-036F,U+2000-206F,U+20AC,U+2122,U+2190-21FF,U+2212,U+2215,U+FEFF,U+FFFD";

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

	const tailwindInput = options.tailwind_input;
	if (typeof tailwindInput !== "string" || tailwindInput.length === 0) {
		fail("plugin options must declare a `tailwind_input` relative to the project root");
	}
	const fontSource = options.font_source;
	if (typeof fontSource !== "string" || fontSource.length === 0) {
		fail("plugin options must declare a `font_source` relative to the project root");
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

	// 1. Minify the split loader and the entry glue with the shared guarded
	// implementation (cargo-leptos' js-minify did both; the leptoskit build
	// emits them unminified). The scratch bundle stays the build's own
	// output: the SDK's bundle report and budget check read it after this
	// hook, so it must remain complete and truthful here.
	const summary = minifySplitLoaders(pkgDir);
	log(`minified ${summary.loaders.length} split loader(s) with ${summary.esbuildBin}`);
	minifyEntryGlue(pkgDir, outputName, summary.esbuildBin);

	// 2. Both frontends compile the same canonical stylesheet; the publisher
	// verifies the two results agree byte for byte before staging.
	const cssPath = join(pkgDir, "monochange_app.css");
	compileTailwind(resolve(root, tailwindInput), cssPath, resolveTailwind(root));

	// 3. Subset the shared fonts to WOFF2 inside the scratch output. The
	// publisher deploys the website build's set; subsetting in both builds
	// keeps the two scratch outputs independently complete.
	if (fontSource.length > 0) {
		const fontDir = join(resolve(outputDir), ".font-subsets");
		mkdirSync(fontDir, { recursive: true });
		for (const name of FONT_NAMES) {
			const source = resolve(root, fontSource, `${name}.ttf`);
			const output = join(fontDir, `${name}.woff2`);
			rmSync(output, { force: true });
			const subset = spawnSync(
				process.env.MONOCHANGE_FONT_SUBSET_BIN ?? "pyftsubset",
				[source, `--output-file=${output}`, "--flavor=woff2", `--unicodes=${FONT_UNICODES}`],
				{ encoding: "utf8" },
			);
			if (subset.error !== undefined || subset.status !== 0 || !existsSync(output)) {
				const detail =
					subset.error?.message || subset.stderr || `exit ${String(subset.status)} or no output`;
				fail(`could not subset ${source}: ${detail}`);
			}
		}
	}

	// 4. Release builds re-optimize the scratch WASM modules in place, so the
	// SDK's own report and budget check measure the deployed bytes.
	if (context.release) {
		const optimized = optimizeWebWasm(pkgDir);
		log(`optimized ${optimized.files} WASM module(s), saving ${optimized.gzipSaved} gzip bytes`);
	}

	log(`finished the ${outputName} build in ${resolve(outputDir)}`);
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
