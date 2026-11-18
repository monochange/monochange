import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, test, vi } from "vitest";

import { optimizeWebWasm } from "../../leptoskit-plugin/optimize-web-wasm.ts";

const emptyModule = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
const roots: string[] = [];

function fixture(mode: string) {
	const root = mkdtempSync(join(tmpdir(), "wasm-optimizer-test-"));
	roots.push(root);
	const input = join(root, "entry.wasm");
	// A valid module with a custom section; the payload has no runtime effect.
	const original = Buffer.concat([emptyModule, Buffer.from([0, 8, 1, 120, 1, 2, 3, 4, 5, 6])]);
	writeFileSync(input, original);
	const tool = join(root, "wasm-opt.mjs");
	writeFileSync(
		tool,
		`#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
const args = process.argv.slice(2);
const original = readFileSync(args.at(-3));
const output = args[args.indexOf("-o") + 1];
const empty = Buffer.from([0,97,115,109,1,0,0,0]);
const mode = ${JSON.stringify(mode)};
if (mode === "fail") { console.error("optimizer diagnostic"); process.exit(2); }
if (mode === "missing") process.exit(0);
const bytes = mode === "invalid" ? Buffer.from([0])
 : mode === "exports" ? Buffer.concat([empty, Buffer.from([5,3,1,0,1,7,7,1,3,109,101,109,2,0])])
 : mode === "larger" ? Buffer.concat([original, Buffer.from([0,8,1,121,7,8,9,10,11,12])])
 : empty;
writeFileSync(output, bytes);
`,
	);
	chmodSync(tool, 0o755);
	vi.stubEnv("MONOCHANGE_WASM_OPT_BIN", tool);

	return { root, input, original };
}

afterEach(() => {
	vi.unstubAllEnvs();
	for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

test("keeps smaller valid modules and converges on repeated builds", () => {
	const { root, input } = fixture("smaller");
	const result = optimizeWebWasm(root);
	assert.equal(result.files, 1);
	assert.ok(result.gzipSaved > 0);
	assert.deepEqual(readFileSync(input), emptyModule);
	assert.deepEqual(optimizeWebWasm(root), { files: 0, gzipSaved: 0 });
});

test("retains the original when optimization increases transferred bytes", () => {
	const { root, input, original } = fixture("larger");
	assert.deepEqual(optimizeWebWasm(root), { files: 0, gzipSaved: 0 });
	assert.deepEqual(readFileSync(input), original);
});

test.each(["fail", "missing", "invalid", "exports"])(
	"fails without replacing the input on %s output",
	(mode) => {
		const { root, input, original } = fixture(mode);
		assert.throws(() => optimizeWebWasm(root));
		assert.deepEqual(readFileSync(input), original);
	},
);
