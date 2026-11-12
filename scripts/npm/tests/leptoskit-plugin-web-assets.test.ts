import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
	chmodSync,
	cpSync,
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
const tailwindInput = join(repoRoot, "app/crates/monochange_app/style/input.css");

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
		},
	});
}

function rootFor(tag: string): string {
	const root = mkdtempSync(join(tmpdir(), `leptoskit-plugin-${tag}-`));

	return root;
}

/** A site root shaped like a finished `leptoskit build --release` output. */
function buildSite(tag: string): { root: string; outDir: string; pkg: string; site: string } {
	const root = rootFor(tag);
	const outDir = join(root, "dist");
	const pkg = join(outDir, "pkg");
	mkdirSync(pkg, { recursive: true });

	for (const name of readdirSync(fixturePkg)) {
		cpSync(join(fixturePkg, name), join(pkg, name));
	}

	// The bound wasm module, which the real build always produces.
	writeFileSync(join(pkg, "monochange_app_bg.wasm"), new Uint8Array([0, 1, 2, 3]));
	// The assembled deployment tree (the website build creates it; both
	// builds assemble into it).
	const site = join(root, "site");

	mkdirSync(site, { recursive: true });
	// A stub Tailwind input shaped like the shared app stylesheet.
	mkdirSync(join(root, "style"), { recursive: true });
	writeFileSync(join(root, "style/input.css"), '@import "tailwindcss";\n');

	return { root, outDir, pkg, site };
}

/** Plugin options shaped like the real [[plugins]] declarations. */
function siteOptions(namespace: string) {
	return { namespace, site_root: "site", tailwind_input: "style/input.css" };
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

	return bin;
}

describe("leptoskit-plugin-monochange-web-assets", () => {
	const roots: string[] = [];

	afterEach(() => {
		for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
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

	test("no-op hooks reply ok without touching the site", () => {
		const { root } = buildSite("noop");
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
	});

	test("after_build minifies, compiles CSS, and namespaces the bundle", () => {
		const { root, outDir, pkg, site } = buildSite("assemble");
		roots.push(root);
		const tailwindBin = fakeTailwind(root);

		const result = runPluginWithTailwind(
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
					options: siteOptions("site"),
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

		const namespaced = join(site, "pkg/site");
		assert.ok(existsSync(join(namespaced, loaderName)));
		// The loader was minified by the shared guarded implementation.
		const loader = readFileSync(join(namespaced, loaderName), "utf8");
		assert.ok(loader.includes("__wasm_split_load___book_route_view"));
		assert.ok(!loader.includes("makeLoad"), "the loader must be minified");
		// The Tailwind output lands beside the entry as `<output_name>.css`.
		const css = readFileSync(join(namespaced, "monochange_app.css"), "utf8");
		assert.match(css, /\/\* tailwind input\.css \*\/\.mc\{color:teal\}/u);
		// Precompressed twins land beside the assembled assets.
		assert.ok(existsSync(join(site, "pkg/site/monochange_app.css.gz")));
		assert.ok(existsSync(join(site, "pkg/site/monochange_app.css.br")));
		assert.ok(existsSync(join(site, "pkg/site/monochange_app.js.gz")));
		const gzipped = readFileSync(join(site, "pkg/site/monochange_app_bg.wasm.gz"));
		assert.ok(gzipped.byteLength > 0);
		// The Tailwind CLI's own stdout must not leak into the reply channel.
		assert.ok(!result.stdout.includes("fake tailwind"));
	});

	test("after_build skips non-web platforms", () => {
		const { root, outDir, pkg, site } = buildSite("native");
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
				options: siteOptions("site"),
			},
		});
		assert.equal(result.status, 0, result.stderr || result.stdout);
		JSON.parse(result.stdout) as { ok: boolean };
		assert.ok(existsSync(pkg), "a non-web build must stay untouched");
	});

	test("after_build fails loudly without a namespace option", () => {
		const { root, outDir, pkg, site } = buildSite("no-namespace");
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
				options: { site_root: "site", tailwind_input: "style/input.css" },
			},
		});
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("namespace"), result.stderr);
		// The failure happened before any site mutation.
		assert.ok(existsSync(pkg));
	});

	test("after_build fails loudly when the tailwind input is missing", () => {
		const { root, outDir, pkg, site } = buildSite("no-input");
		roots.push(root);
		rmSync(join(root, "style/input.css"));
		const tailwindBin = fakeTailwind(root);
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
					options: { namespace: "site", site_root: "site", tailwind_input: "style/missing.css" },
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
		const { root, outDir, pkg, site } = buildSite("no-tailwind");
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
					options: siteOptions("site"),
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

	test("after_build fails loudly when the loader is missing", () => {
		const root = rootFor("no-loader");
		roots.push(root);
		const outDir = join(root, "dist");
		const pkg = join(outDir, "pkg");
		mkdirSync(pkg, { recursive: true });
		mkdirSync(join(root, "site"), { recursive: true });
		writeFileSync(join(pkg, "monochange_app_bg.wasm"), new Uint8Array([0]));
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
					options: { namespace: "site", site_root: "site", tailwind_input: join(tailwindInput) },
				},
			},
			root,
		);
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("__wasm_split"), result.stderr);
	});

	test("the plugin runs from any working directory", () => {
		const { root, outDir, pkg, site } = buildSite("cwd");
		roots.push(root);
		const tailwindBin = fakeTailwind(root);
		mkdirSync(join(site, "pkg/projects"), { recursive: true });
		for (const suffix of ["", ".gz", ".br"]) {
			writeFileSync(join(site, `pkg/projects/monochange_app.css${suffix}`), "stale");
		}
		// Relative paths in the envelope and the tailwind_input resolve
		// against the project root in the envelope, not the cwd.
		const result = runPluginWithTailwind(
			{
				protocol: 1,
				plugin: "monochange-web-assets",
				hook: "after_build",
				context: {
					root,
					platform: "web",
					release: true,
					output_dir: "dist",
					wasm_bundle: "dist/pkg/monochange_app_bg.wasm",
					options: siteOptions("projects"),
				},
			},
			tailwindBin,
			root,
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const css = readFileSync(join(site, "pkg/site/monochange_app.css"), "utf8");
		assert.ok(css.length > 0);
		for (const suffix of ["", ".gz", ".br"]) {
			assert.ok(!existsSync(join(site, `pkg/projects/monochange_app.css${suffix}`)));
		}
		assert.ok(tailwindBin.length > 0);
	});

	test("the PATH shim resolves the plugin through symlinks from any cwd", () => {
		const { root, outDir, pkg, site } = buildSite("shim");
		roots.push(root);
		const tailwindBin = fakeTailwind(root);
		const link = join(root, "bin");
		mkdirSync(link, { recursive: true });
		symlinkSync(shimPath, join(link, "leptoskit-plugin-monochange-web-assets"));
		const result = spawnSync("leptoskit-plugin-monochange-web-assets", [], {
			input: `${JSON.stringify({
				protocol: 1,
				plugin: "monochange-web-assets",
				hook: "describe",
				context: {},
			})}\n`,
			encoding: "utf8",
			cwd: root,
			env: {
				...process.env,
				PATH: `${link}:${process.env.PATH ?? ""}`,
				MONOCHANGE_TAILWIND_BIN: tailwindBin,
				MONOCHANGE_ESBUILD_BIN: esbuildBin,
			},
		});
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const reply = JSON.parse(result.stdout) as { ok: boolean };
		assert.equal(reply.ok, true);
		// And the shim reaches the real plugin for the working hook too.
		const assembled = spawnSync("leptoskit-plugin-monochange-web-assets", [], {
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
					options: siteOptions("site"),
				},
			})}\n`,
			encoding: "utf8",
			cwd: root,
			env: {
				...process.env,
				PATH: `${link}:${process.env.PATH ?? ""}`,
				MONOCHANGE_TAILWIND_BIN: tailwindBin,
				MONOCHANGE_ESBUILD_BIN: esbuildBin,
			},
		});
		assert.equal(assembled.status, 0, assembled.stderr || assembled.stdout);
		assert.ok(existsSync(join(site, "pkg/site/monochange_app.css")));
	});

	test("rejects a foreign protocol or plugin name", () => {
		const result = runPlugin({ protocol: 99, plugin: "other", hook: "describe", context: {} });
		assert.equal(result.status, 1);
		assert.ok(result.stderr.includes("protocol"), result.stderr);
	});
});
