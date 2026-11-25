import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, test } from "vitest";

import { checkExternrefTable } from "../../check-wasm-externref-table.ts";

const script = resolve(
	dirname(fileURLToPath(import.meta.url)),
	"../../check-wasm-externref-table.ts",
);
const FUNCREF = 0x70;
const EXTERNREF = 0x6f;
const KIND_TABLE = 0x01;
const KIND_MEMORY = 0x02;

type Table = { element: number; min: number; max?: number };
type Export = { name: string; kind: number; index: number };

function uleb(value: number): number[] {
	const bytes: number[] = [];

	do {
		let byte = value & 0x7f;
		value = Math.floor(value / 128);
		if (value !== 0) byte |= 0x80;
		bytes.push(byte);
	} while (value !== 0);

	return bytes;
}

function name(text: string): number[] {
	const bytes = [...Buffer.from(text, "utf8")];

	return [...uleb(bytes.length), ...bytes];
}

function vector(items: number[][]): number[] {
	return [...uleb(items.length), ...items.flat()];
}

function limits({ min, max }: Table): number[] {
	return max === undefined ? [0x00, ...uleb(min)] : [0x01, ...uleb(min), ...uleb(max)];
}

function section(id: number, payload: number[]): number[] {
	return [id, ...uleb(payload.length), ...payload];
}

/** A minimal module: optional imported tables, defined tables, a memory, and exports. */
function module(options: {
	imported?: Table[];
	tables: Table[];
	exports: Export[];
}): Uint8Array<ArrayBuffer> {
	const imports = (options.imported ?? []).map((table, index) => [
		...name("env"),
		...name(`table${index}`),
		KIND_TABLE,
		table.element,
		...limits(table),
	]);
	const sections = [
		...(imports.length > 0 ? section(2, vector(imports)) : []),
		...section(4, vector(options.tables.map((table) => [table.element, ...limits(table)]))),
		...section(5, vector([[0x00, 0x01]])),
		...section(
			7,
			vector(
				options.exports.map((entry) => [...name(entry.name), entry.kind, ...uleb(entry.index)]),
			),
		),
	];

	return new Uint8Array([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, ...sections]);
}

/** The shape wasm-split plus wasm-bindgen produce: a fixed funcref table and a growable externref table. */
function bindgenModule(
	externrefsIndex: number,
	externref: Table = { element: EXTERNREF, min: 1024 },
) {
	return module({
		tables: [{ element: FUNCREF, min: 3284, max: 3284 }, externref],
		exports: [
			{ name: "__indirect_function_table", kind: KIND_TABLE, index: 0 },
			{ name: "__wbindgen_externrefs", kind: KIND_TABLE, index: externrefsIndex },
		],
	});
}

function valid(bytes: Uint8Array<ArrayBuffer>): Uint8Array<ArrayBuffer> {
	// Every fixture that claims to be a module must be one the engine accepts.
	new WebAssembly.Module(bytes);

	return bytes;
}

describe("checkExternrefTable", () => {
	test("accepts the export on the growable externref table", () => {
		const result = checkExternrefTable(valid(bindgenModule(1)));
		assert.equal(result.ok, true, result.message);
		assert.deepEqual(result.exported, {
			index: 1,
			table: { source: "defined", element: "externref", min: 1024, max: null },
		});
	});

	test("rejects the release-image shape: the export on the fixed funcref table", () => {
		const result = checkExternrefTable(valid(bindgenModule(0)));
		assert.equal(result.ok, false);
		assert.match(result.message, /-> table 0 \(defined funcref min=3284 max=3284\)/);
		assert.match(result.message, /element type must be externref/);
		assert.equal(result.tables.length, 2);
	});

	test.each([
		[1027, false],
		[1028, true],
	])("with an externref maximum of %i, growth by 4 passes: %s", (max, ok) => {
		const result = checkExternrefTable(
			valid(bindgenModule(1, { element: EXTERNREF, min: 1024, max })),
		);
		assert.equal(result.ok, ok, result.message);
		if (!ok) assert.match(result.message, /grows it by 4, which the maximum forbids/);
	});

	test("counts imported tables before defined ones", () => {
		const imported = [{ element: FUNCREF, min: 1, max: 1 }];
		const tables = [{ element: EXTERNREF, min: 0 }];
		const exportAt = (index: number) =>
			module({
				imported,
				tables,
				exports: [{ name: "__wbindgen_externrefs", kind: KIND_TABLE, index }],
			});

		const onDefined = checkExternrefTable(valid(exportAt(1)));
		assert.equal(onDefined.ok, true, onDefined.message);
		assert.equal(onDefined.exported?.table.source, "defined");

		const onImported = checkExternrefTable(valid(exportAt(0)));
		assert.equal(onImported.ok, false);
		assert.match(onImported.message, /table 0 \(import env\.table0 funcref min=1 max=1\)/);
	});

	test("rejects a module without the export", () => {
		const bytes = valid(module({ tables: [{ element: EXTERNREF, min: 0 }], exports: [] }));
		const result = checkExternrefTable(bytes);
		assert.equal(result.ok, false);
		assert.equal(result.message, 'no "__wbindgen_externrefs" export');
	});

	test("rejects the export when it is not a table", () => {
		const bytes = valid(
			module({
				tables: [{ element: EXTERNREF, min: 0 }],
				exports: [{ name: "__wbindgen_externrefs", kind: KIND_MEMORY, index: 0 }],
			}),
		);
		const result = checkExternrefTable(bytes);
		assert.equal(result.ok, false);
		assert.match(result.message, /exported, but not as a table/);
	});

	test("rejects an export index past the table index space", () => {
		const result = checkExternrefTable(bindgenModule(2));
		assert.equal(result.ok, false);
		assert.match(result.message, /refers to table 2, but the module has 2 table\(s\)/);
	});

	test.each([
		[
			"not a module",
			new Uint8Array([0x7f, 0x45, 0x4c, 0x46, 1, 0, 0, 0]),
			/not a WebAssembly 1\.0/,
		],
		[
			"a truncated section",
			bindgenModule(1).subarray(0, 20),
			/runs past its section|unexpected end/,
		],
	])("reports %s as unreadable", (_label, bytes, pattern) => {
		const result = checkExternrefTable(bytes);
		assert.equal(result.ok, false);
		assert.match(result.message, /^unreadable module: /);
		assert.match(result.message, pattern);
	});
});

describe("CLI", () => {
	const roots: string[] = [];

	afterEach(() => {
		for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
	});

	function write(files: Record<string, Uint8Array>): string[] {
		const root = mkdtempSync(join(tmpdir(), "externref-table-test-"));
		roots.push(root);

		return Object.entries(files).map(([file, bytes]) => {
			const path = join(root, file);
			writeFileSync(path, bytes);

			return path;
		});
	}

	function run(args: string[]) {
		return spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });
	}

	test("passes when every module is correct", () => {
		const paths = write({ "site.wasm": bindgenModule(1), "projects.wasm": bindgenModule(1) });
		const result = run(paths);
		assert.equal(result.status, 0, result.stderr);
		assert.equal(result.stdout.match(/^ok {3}/gm)?.length, 2);
	});

	test("fails and lists the tables when any module is broken", () => {
		const paths = write({ "site.wasm": bindgenModule(1), "projects.wasm": bindgenModule(0) });
		const result = run(paths);
		assert.equal(result.status, 1);
		assert.match(result.stdout, /^ok {3}.*site\.wasm/m);
		assert.match(result.stderr, /^FAIL .*projects\.wasm: .*element type must be externref/m);
		assert.match(result.stderr, /table 1 \(defined externref min=1024 no max\)/);
	});

	test("requires at least one module", () => {
		const result = run([]);
		assert.equal(result.status, 2);
		assert.match(result.stderr, /^usage: /);
	});
});
