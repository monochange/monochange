import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
	chmodSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	readdirSync,
	rmSync,
	symlinkSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, test } from "vitest";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const pluginPath = join(repoRoot, "scripts/leptoskit-plugin/web-assets-plugin.ts");
const shimPath = join(repoRoot, "scripts/leptoskit-plugin/leptoskit-plugin-monochange-web-assets");
const esbuildBin = join(repoRoot, "node_modules/.bin/esbuild");
const loaderName = "__wasm_split.______________________.js";
const fixturePkg = join(repoRoot, "fixtures/tests/website-split-loader-minify/loader-site/pkg");

function runPlugin(envelope: unknown, cwd = repoRoot) {
	return spawnSync(process.execPath, [pluginPath], {
		input: `${JSON.stringify(envelope)}\n`,
		encoding: "utf8",
		cwd,
		env: { ...process.env, MONOCHANGE_ESBUILD_BIN: esbuildBin },
	});
}

/** Runs the plugin with a fake Tailwind CLI, as the real build environment does. */
function runPluginWithTailwind(envelope: unknown, tailwindBin: string, cwd?: string) {
	return spawnSync(process.execPath, [pluginPath], {
		input: `${JSON.stringify(envelope)}\n`,
		encoding: "utf8",
		cwd: cwd ?? repoRoot,
		env: {
			...process.env,
			MONOCHANGE_ESBUILD_BIN: esbuildBin,
			MONOCHANGE_TAILWIND_BIN: tailwindBin,
			MONOCHANGE_FONT_SUBSET_BIN: join(dirname(tailwindBin), "font-subset-fake.mjs"),
			MONOCHANGE_WASM_OPT_BIN: join(dirname(tailwindBin), "wasm-opt-fake.mjs"),
		},
	});
}

function rootFor(tag: string): string {
	return mkdtempSync(join(tmpdir(), `leptoskit-plugin-${tag}-`));
}

/** A scratch build output shaped like a finished `leptoskit build` run. */
function buildScratch(tag: string): { root: string; outDir: string; pkg: string } {
	const root = rootFor(tag);
	const outDir = join(root, "scratch");
	const pkg = join(outDir, "pkg");
	mkdirSync(pkg, { recursive: true });

	for (const name of readdirSync(fixturePkg)) {
		cpFile(join(fixturePkg, name), join(pkg, name));
	}

	// The bound wasm module, which the real build always produces.
	writeFileSync(join(pkg, "monochange_app_bg.wasm"), wasmBytes());

	// A stub Tailwind input and font sources shaped like the app workspace.
	mkdirSync(join(root, "style"), { recursive: true });
	writeFileSync(join(root, "style/input.css"), '@import "tailwindcss";\n');
	mkdirSync(join(root, "fonts"), { recursive: true });
	for (const name of ["bricolage-800", "hanken-400", "hanken-700"]) {
		writeFileSync(join(root, "fonts", `${name}.ttf`), "fixture font");
	}

	return { root, outDir, pkg };
}

function cpFile(from: string, to: string): void {
	if (existsSync(to)) return;
	writeFileSync(to, readFileSync(from));
}

/** The eight-byte header every fixture wasm module needs to stay parseable. */
function wasmBytes(): Buffer {
	return Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
}

/** Plugin options shaped like the real [[plugins]] declarations. */
function pluginOptions() {
	return { tailwind_input: "style/input.css", font_source: "fonts" };
}

/** A tiny fake Tailwind CLI that writes deterministic minified CSS. */
function fakeTailwind(root: string): string {
	const bin = join(root, "tailwindcss-fake.mjs");
	writeFileSync(
		bin,
		[
			"#!/usr/bin/env node",
			'import { writeFileSync, mkdirSync } from "node:fs";',
			'import { dirname } from "node:path";',
			"const args = process.argv.slice(2);",
			'const input = args[args.indexOf("-i") + 1];',
			'const output = args[args.indexOf("-o") + 1];',
			"mkdirSync(dirname(output), { recursive: true });",
			'writeFileSync(output, `/* tailwind ${input.split("/").at(-1)} */.mc{color:teal}`);',
			'console.log("fake tailwind progress line");',
		].join("\n"),
	);
	chmodSync(bin, 0o755);
	const fontBin = join(root, "font-subset-fake.mjs");
	writeFileSync(
		fontBin,
		[
			"#!/usr/bin/env node",
			'import { writeFileSync } from "node:fs";',
			'const output = process.argv.find((arg) => arg.startsWith("--output-file="));',
			'writeFileSync(output.slice("--output-file=".length), "fixture woff2");',
		].join("\n"),
	);
	chmodSync(fontBin, 0o755);
	const wasmBin = join(root, "wasm-opt-fake.mjs");
	writeFileSync(
		wasmBin,
		[
			"#!/usr/bin/env node",
			'import { writeFileSync } from "node:fs";',
			"const args = process.argv.slice(2);",
			'writeFileSync(args[args.indexOf("-o") + 1], Buffer.from([0,97,115,109,1,0,0,0]));',
		].join("\n"),
	);
	chmodSync(wasmBin, 0o755);

	return bin;
}

describe("leptoskit-plugin-monochange-web-assets", () => {
	const roots: string[] = [];

	afterEach(() => {
		for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
	});

	test("after_build finishes inside the scratch output and never publishes", () => {
		const { root, outDir, pkg } = buildScratch("in-place");
		roots.push(root);
		const tailwindBin = fakeTailwind(root);
		const result = runPluginWithTailwind(
			{
				protocol: 1,
				plugin: "monochange-web-assets",
				hook: "after_build",
				context: {
					platform: "web",
					release: false,
					output_dir: outDir,
					wasm_bundle: `${pkg}/monochange_app_bg.wasm`,
					options: pluginOptions(),
				},
			},
			tailwindBin,
			root,
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);

		// stdout is only the JSON reply: plugin and tool logs stay on stderr.
		const reply = JSON.parse(result.stdout) as { ok: boolean; protocol: number };
		assert.equal(reply.ok, true);
		assert.equal(reply.protocol, 1);

		// The guarded minifier ran inside the scratch pkg.
		const loader = readFileSync(join(pkg, loaderName), "utf8");
		assert.ok(loader.includes("__wasm_split_load___book_route_view"));
		assert.ok(!loader.includes("makeLoad"), "the loader must be minified");
		// The shared stylesheet compiled beside the entry, in the scratch only.
		const css = readFileSync(join(pkg, "monochange_app.css"), "utf8");
		assert.match(css, /\/\* tailwind input\.css \*\/\.mc\{color:teal\}/u);
		// The WOFF2 subsets landed in the scratch output's hidden directory.
		assert.equal(
			readFileSync(join(outDir, ".font-subsets/hanken-400.woff2"), "utf8"),
			"fixture woff2",
		);

		// No publication happened: no site root, no assembled namespaces, and
		// the scratch bundle is still complete for the SDK's own report.
		// (The fake tool bins beside the fixture are the test's own.)
		assert.deepEqual(readdirSync(root).toSorted(), [
			"font-subset-fake.mjs",
			"fonts",
			"scratch",
			"style",
			"tailwindcss-fake.mjs",
			"wasm-opt-fake.mjs",
		]);
		assert.ok(existsSync(join(pkg, "monochange_app_bg.wasm")));
		assert.ok(existsSync(join(pkg, "monochange_app.js")));
		assert.ok(existsSync(join(pkg, "__wasm_split_manifest.json")));
		assert.ok(!existsSync(join(root, "site")));
		assert.ok(!existsSync(join(pkg, "monochange_app.css.gz")));
		// The Tailwind CLI's own stdout must not leak into the reply channel.
		assert.ok(!result.stdout.includes("fake tailwind"));
	});

	test("release builds optimize the scratch wasm before the SDK's budget check", () => {
		const { root, outDir, pkg } = buildScratch("wasm-compression");
		roots.push(root);
		const tailwind = fakeTailwind(root);
		const empty = wasmBytes();
		const original = Buffer.concat([empty, Buffer.from([0, 8, 1, 120, 1, 2, 3, 4, 5, 6])]);
		writeFileSync(join(pkg, "monochange_app_bg.wasm"), original);
		for (const release of [false, true]) {
			writeFileSync(join(pkg, "monochange_app_bg.wasm"), original);
			const result = runPluginWithTailwind(
				{
					protocol: 1,
					plugin: "monochange-web-assets",
					hook: "after_build",
					context: {
						platform: "web",
						release,
						output_dir: outDir,
						wasm_bundle: join(pkg, "monochange_app_bg.wasm"),
						options: pluginOptions(),
					},
				},
				tailwind,
				root,
			);
			assert.equal(result.status, 0, result.stderr);
			const scratchWasm = readFileSync(join(pkg, "monochange_app_bg.wasm"));
			assert.deepEqual(
				scratchWasm,
				release ? empty : original,
				`${release ? "release" : "debug"} scratch wasm must be the ${release ? "optimized" : "original"} bytes`,
			);
		}
	});

	test("describes itself over the protocol", () => {
		const result = runPlugin({
			protocol: 1,
			plugin: "monochange-web-assets",
			hook: "describe",
			context: {},
		});
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const reply = JSON.parse(result.stdout) as { ok: boolean; platforms: string[] };
		assert.equal(reply.ok, true);
		// Empty platforms: the plugin supports every platform and only acts on
		// web builds, so server-only leptoskit invocations still resolve it.
		assert.deepEqual(reply.platforms, []);
	});

	test("no-op hooks reply ok without touching anything", () => {
		const { root, outDir, pkg } = buildScratch("noop");
		roots.push(root);
		for (const hook of ["generate", "before_build", "before_serve"]) {
			const result = runPlugin({
				protocol: 1,
				plugin: "monochange-web-assets",
				hook,
				context: { root },
			});
			assert.equal(result.status, 0, result.stderr || result.stdout);
			const reply = JSON.parse(result.stdout) as { ok: boolean };
			assert.equal(reply.ok, true);
		}
		assert.ok(existsSync(pkg), "a no-op hook must leave the scratch untouched");
		assert.ok(!existsSync(join(outDir, ".font-subsets")));
	});

	test("after_build skips non-web platforms", () => {
		const { root, outDir, pkg } = buildScratch("native");
		roots.push(root);
		const result = runPlugin({
			protocol: 1,
			plugin: "monochange-web-assets",
			hook: "after_build",
			context: {
				root,
				platform: "android",
				release: false,
				output_dir: outDir,
				wasm_bundle: null,
				options: pluginOptions(),
			},
		});
		assert.equal(result.status, 0, result.stderr || result.stdout);
		JSON.parse(result.stdout) as { ok: boolean };
		assert.ok(existsSync(pkg), "a non-web build must stay untouched");
	});

	test("after_build fails loudly without the tailwind_input option", () => {
		const { root, outDir, pkg } = buildScratch("no-namespace");
		roots.push(root);
		const result = runPlugin({
			protocol: 1,
			plugin: "monochange-web-assets",
			hook: "after_build",
			context: {
				root,
				platform: "web",
				release: true,
				output_dir: outDir,
				wasm_bundle: `${pkg}/monochange_app_bg.wasm`,
				options: { font_source: "fonts" },
			},
		});
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("tailwind_input"), result.stderr);
		// The failure happened before any mutation.
		assert.ok(existsSync(pkg));
		assert.ok(!existsSync(join(outDir, ".font-subsets")));
	});

	test("after_build fails loudly without the font_source option", () => {
		const { root, outDir, pkg } = buildScratch("no-font-option");
		roots.push(root);
		const result = runPlugin({
			protocol: 1,
			plugin: "monochange-web-assets",
			hook: "after_build",
			context: {
				root,
				platform: "web",
				release: true,
				output_dir: outDir,
				wasm_bundle: `${pkg}/monochange_app_bg.wasm`,
				options: { tailwind_input: "style/input.css" },
			},
		});
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("font_source"), result.stderr);
	});

	test("after_build fails loudly when the tailwind input is missing", () => {
		const { root, outDir, pkg } = buildScratch("no-input");
		roots.push(root);
		const tailwindBin = fakeTailwind(root);
		rmSync(join(root, "style/input.css"));
		const result = runPlugin(
			{
				protocol: 1,
				plugin: "monochange-web-assets",
				hook: "after_build",
				context: {
					root,
					platform: "web",
					release: true,
					output_dir: outDir,
					wasm_bundle: `${pkg}/monochange_app_bg.wasm`,
					options: { tailwind_input: "style/missing.css", font_source: "fonts" },
				},
			},
			root,
		);
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("missing.css"), result.stderr);
		assert.ok(existsSync(pkg), "a failed hook must leave the bundle in place");
		assert.ok(tailwindBin.length > 0);
	});

	test("after_build fails loudly when no tailwind compiler exists", () => {
		const { root, outDir, pkg } = buildScratch("no-tailwind");
		roots.push(root);
		const result = spawnSync(process.execPath, [pluginPath], {
			input: `${JSON.stringify({
				protocol: 1,
				plugin: "monochange-web-assets",
				hook: "after_build",
				context: {
					root,
					platform: "web",
					release: true,
					output_dir: outDir,
					wasm_bundle: `${pkg}/monochange_app_bg.wasm`,
					options: pluginOptions(),
				},
			})}\n`,
			encoding: "utf8",
			cwd: root,
			env: {
				...process.env,
				MONOCHANGE_ESBUILD_BIN: esbuildBin,
				MONOCHANGE_TAILWIND_BIN: join(root, "no-such-tailwind"),
				PATH: "/usr/bin:/bin",
			},
		});
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("MONOCHANGE_TAILWIND_BIN"), result.stderr);
	});

	test("after_build fails when the font subsetter cannot run", () => {
		const { root, outDir, pkg } = buildScratch("no-font-subsetter");
		roots.push(root);
		const tailwindBin = fakeTailwind(root);
		rmSync(join(root, "font-subset-fake.mjs"));
		const result = runPluginWithTailwind(
			{
				protocol: 1,
				plugin: "monochange-web-assets",
				hook: "after_build",
				context: {
					platform: "web",
					output_dir: outDir,
					wasm_bundle: `${pkg}/monochange_app_bg.wasm`,
					options: pluginOptions(),
				},
			},
			tailwindBin,
			root,
		);
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("could not subset"), result.stderr);
		assert.ok(existsSync(pkg), "a failed hook must leave the bundle in place");
	});

	test("after_build rejects a successful subsetter that leaves only stale output", () => {
		const { root, outDir, pkg } = buildScratch("stale-font");
		roots.push(root);
		const tailwindBin = fakeTailwind(root);
		writeFileSync(join(root, "font-subset-fake.mjs"), "#!/usr/bin/env node\n");
		mkdirSync(join(outDir, ".font-subsets"), { recursive: true });
		writeFileSync(join(outDir, ".font-subsets/bricolage-800.woff2"), "stale");
		const result = runPluginWithTailwind(
			{
				protocol: 1,
				plugin: "monochange-web-assets",
				hook: "after_build",
				context: {
					platform: "web",
					output_dir: outDir,
					wasm_bundle: `${pkg}/monochange_app_bg.wasm`,
					options: pluginOptions(),
				},
			},
			tailwindBin,
			root,
		);
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("no output"), result.stderr);
	});

	test("after_build fails loudly when the loader is missing", () => {
		const { root, outDir, pkg } = buildScratch("no-loader");
		roots.push(root);
		rmSync(join(pkg, loaderName));
		const result = runPlugin(
			{
				protocol: 1,
				plugin: "monochange-web-assets",
				hook: "after_build",
				context: {
					root,
					platform: "web",
					release: true,
					output_dir: outDir,
					wasm_bundle: `${pkg}/monochange_app_bg.wasm`,
					options: pluginOptions(),
				},
			},
			root,
		);
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("__wasm_split"), result.stderr);
	});

	test("after_build runs from any working directory", () => {
		const { root, outDir, pkg } = buildScratch("cwd");
		roots.push(root);
		const tailwindBin = fakeTailwind(root);
		// Relative paths in the envelope resolve against the project root in
		// the envelope's cwd, not the publisher's working directory.
		const result = runPluginWithTailwind(
			{
				protocol: 1,
				plugin: "monochange-web-assets",
				hook: "after_build",
				context: {
					root,
					platform: "web",
					release: false,
					output_dir: "scratch",
					wasm_bundle: "scratch/pkg/monochange_app_bg.wasm",
					options: pluginOptions(),
				},
			},
			tailwindBin,
			root,
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		assert.ok(existsSync(join(pkg, "monochange_app.css")));
		assert.ok(tailwindBin.length > 0);
	});

	test("the PATH shim resolves the plugin through symlinks from any cwd", () => {
		const { root, outDir, pkg } = buildScratch("shim");
		roots.push(root);
		const tailwindBin = fakeTailwind(root);
		const link = join(root, "bin");
		mkdirSync(link, { recursive: true });
		symlinkSync(shimPath, join(link, "leptoskit-plugin-monochange-web-assets"));
		const run = (hook: string, extra: Record<string, unknown>) =>
			spawnSync("leptoskit-plugin-monochange-web-assets", [], {
				input: `${JSON.stringify({
					protocol: 1,
					plugin: "monochange-web-assets",
					hook,
					context: extra,
				})}\n`,
				encoding: "utf8",
				cwd: root,
				env: {
					...process.env,
					PATH: `${link}:${process.env.PATH ?? ""}`,
					MONOCHANGE_TAILWIND_BIN: tailwindBin,
					MONOCHANGE_FONT_SUBSET_BIN: join(root, "font-subset-fake.mjs"),
					MONOCHANGE_ESBUILD_BIN: esbuildBin,
					MONOCHANGE_WASM_OPT_BIN: join(root, "wasm-opt-fake.mjs"),
				},
			});
		const described = run("describe", {});
		assert.equal(described.status, 0, described.stderr || described.stdout);
		const reply = JSON.parse(described.stdout) as { ok: boolean };
		assert.equal(reply.ok, true);
		// And the shim reaches the real plugin for the working hook too.
		const assembled = run("after_build", {
			root,
			platform: "web",
			release: true,
			output_dir: outDir,
			wasm_bundle: `${pkg}/monochange_app_bg.wasm`,
			options: pluginOptions(),
		});
		assert.equal(assembled.status, 0, assembled.stderr || assembled.stdout);
		assert.ok(existsSync(join(pkg, "monochange_app.css")));
	});

	test("rejects a foreign protocol or plugin name", () => {
		const result = runPlugin({ protocol: 99, plugin: "other", hook: "describe", context: {} });
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("protocol"), result.stderr);
	});
});
