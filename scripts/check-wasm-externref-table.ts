#!/usr/bin/env node
// Verifies that a wasm-bindgen main module exports a growable externref table
// as `__wbindgen_externrefs`.
//
// The wasm-bindgen glue initializes the externref heap before anything else
// runs: `const table = wasm.__wbindgen_externrefs; table.grow(4); ...`. When a
// build step maps that export to the wrong table (for example the fixed-size
// funcref table that wasm-split exports as `__indirect_function_table`), the
// server still serves every page and asset with 200, but hydration throws
// `RangeError: WebAssembly.Table.grow(): failed to grow table by 4` in every
// browser. Only the module's own sections show the fault, so this reads them.
//
// The reader is deliberately small: it walks the import, table, and export
// sections, resolves the export's index in the table index space (imported
// tables first, then defined ones), and checks the table's element type and
// limits. Every other section is skipped by its declared size.
//
// Usage:
//   node scripts/check-wasm-externref-table.ts <main_bg.wasm>...
import { readFileSync, realpathSync } from "node:fs";
import process from "node:process";
import { pathToFileURL } from "node:url";

/** The export wasm-bindgen's glue grows and fills with its initial externrefs. */
export const EXTERNREF_EXPORT = "__wbindgen_externrefs";

/** How many slots the glue's initialization grows the externref table by. */
export const REQUIRED_GROWTH = 4;

const WASM_MAGIC = [0x00, 0x61, 0x73, 0x6d];
const WASM_VERSION = [0x01, 0x00, 0x00, 0x00];

const SECTION_IMPORT = 2;
const SECTION_TABLE = 4;
const SECTION_EXPORT = 7;

const EXTERNAL_FUNCTION = 0x00;
const EXTERNAL_TABLE = 0x01;
const EXTERNAL_MEMORY = 0x02;
const EXTERNAL_GLOBAL = 0x03;
const EXTERNAL_TAG = 0x04;

const REF_NULL = 0x63;
const REF = 0x64;
const HEAP_EXTERN = 0x6f;
const HEAP_FUNC = 0x70;
const TABLE_WITH_INIT = 0x40;

const LIMITS_HAS_MAX = 0x01;
const LIMITS_SHARED = 0x02;
const LIMITS_64 = 0x04;
const LIMITS_KNOWN = LIMITS_HAS_MAX | LIMITS_SHARED | LIMITS_64;

/** One table in the module's table index space. */
export type WasmTable = {
	/** `import <module>.<name>` or `defined`. */
	source: string;
	/** `externref`, `funcref`, or the raw reference type. */
	element: string;
	min: number;
	/** `null` when the table declares no maximum. */
	max: number | null;
};

/** The outcome of checking one module. */
export type ExternrefTableCheck = {
	ok: boolean;
	message: string;
	tables: WasmTable[];
	/** The table the export refers to, when the export exists and resolves. */
	exported: { index: number; table: WasmTable } | null;
};

class WasmFormatError extends Error {}

class Reader {
	position = 0;
	private readonly bytes: Uint8Array;
	private readonly end: number;

	constructor(bytes: Uint8Array, end: number = bytes.length) {
		this.bytes = bytes;
		this.end = end;
	}

	get done(): boolean {
		return this.position >= this.end;
	}

	byte(): number {
		if (this.position >= this.end) {
			throw new WasmFormatError(`unexpected end of module at byte ${this.position}`);
		}

		return this.bytes[this.position++];
	}

	peek(): number {
		const byte = this.byte();
		this.position--;

		return byte;
	}

	/** An unsigned LEB128 integer up to 2^53 (table64 limits fit). */
	unsigned(): number {
		let result = 0;
		let scale = 1;

		for (;;) {
			const byte = this.byte();
			result += (byte & 0x7f) * scale;

			if ((byte & 0x80) === 0) break;
			scale *= 128;

			if (scale > 2 ** 56) {
				throw new WasmFormatError(`LEB128 integer too long at byte ${this.position}`);
			}
		}

		return result;
	}

	/** Skips one signed LEB128 integer (a heap type index). */
	skipSigned(): void {
		while ((this.byte() & 0x80) !== 0);
	}

	name(): string {
		const length = this.unsigned();
		const start = this.position;
		this.skip(length);

		return Buffer.from(this.bytes.subarray(start, start + length)).toString("utf8");
	}

	skip(length: number): void {
		if (this.position + length > this.end) {
			throw new WasmFormatError(`a field at byte ${this.position} runs past its section`);
		}

		this.position += length;
	}

	section(size: number): Reader {
		const start = this.position;
		this.skip(size);
		const reader = new Reader(this.bytes, start + size);
		reader.position = start;

		return reader;
	}

	/** A reference type: the one-byte shorthands or `(ref null? <heap type>)`. */
	referenceType(): string {
		const type = this.byte();

		if (type === HEAP_EXTERN) return "externref";
		if (type === HEAP_FUNC) return "funcref";

		if (type === REF_NULL || type === REF) {
			const heap = this.bytes[this.position];
			this.skipSigned();
			const nullable = type === REF_NULL;

			if (nullable && heap === HEAP_EXTERN) return "externref";
			if (nullable && heap === HEAP_FUNC) return "funcref";

			return `(ref ${nullable ? "null " : ""}0x${heap.toString(16)})`;
		}

		if (type >= 0x69 && type <= 0x74) return `0x${type.toString(16)}`;

		throw new WasmFormatError(`unknown reference type 0x${type.toString(16)}`);
	}

	/** A value type, as a global's type: numeric, vector, or reference. */
	skipValueType(): void {
		const type = this.bytes[this.position];

		if (type >= 0x7b && type <= 0x7f) {
			this.position++;

			return;
		}

		this.referenceType();
	}

	limits(): { min: number; max: number | null } {
		const flags = this.byte();

		if ((flags & ~LIMITS_KNOWN) !== 0) {
			throw new WasmFormatError(`unknown limits flags 0x${flags.toString(16)}`);
		}

		const min = this.unsigned();
		const max = (flags & LIMITS_HAS_MAX) !== 0 ? this.unsigned() : null;

		return { min, max };
	}
}

function readHeader(reader: Reader): void {
	const header = [...WASM_MAGIC, ...WASM_VERSION];

	for (const expected of header) {
		if (reader.byte() !== expected) {
			throw new WasmFormatError("not a WebAssembly 1.0 binary module");
		}
	}
}

function readImports(section: Reader, tables: WasmTable[]): void {
	const count = section.unsigned();

	for (let index = 0; index < count; index++) {
		const module = section.name();
		const name = section.name();
		const kind = section.byte();

		switch (kind) {
			case EXTERNAL_FUNCTION: {
				section.unsigned();
				break;
			}
			case EXTERNAL_TABLE: {
				const element = section.referenceType();
				tables.push({ source: `import ${module}.${name}`, element, ...section.limits() });
				break;
			}
			case EXTERNAL_MEMORY: {
				section.limits();
				break;
			}
			case EXTERNAL_GLOBAL: {
				section.skipValueType();
				section.byte();
				break;
			}
			case EXTERNAL_TAG: {
				section.byte();
				section.unsigned();
				break;
			}
			default: {
				throw new WasmFormatError(
					`unknown import kind 0x${kind.toString(16)} for ${module}.${name}`,
				);
			}
		}
	}
}

function readTables(section: Reader, tables: WasmTable[]): void {
	const count = section.unsigned();

	for (let index = 0; index < count; index++) {
		// A table with an initializer expression (`0x40 0x00 type limits expr`)
		// ends what this reader can follow: the constant expression carries no
		// length prefix. It is fine as the last table; anything after it is
		// out of reach, so refuse rather than misnumber the tables.
		const withInit = section.peek() === TABLE_WITH_INIT;

		if (withInit) section.skip(2);

		const element = section.referenceType();
		tables.push({ source: "defined", element, ...section.limits() });

		if (withInit && index + 1 < count) {
			throw new WasmFormatError("a table with an initializer expression precedes other tables");
		}

		if (withInit) return;
	}
}

function readTableExport(section: Reader, name: string): number | "missing" | "not-a-table" {
	const count = section.unsigned();

	for (let index = 0; index < count; index++) {
		const exportName = section.name();
		const kind = section.byte();
		const target = section.unsigned();

		if (exportName === name) {
			return kind === EXTERNAL_TABLE ? target : "not-a-table";
		}
	}

	return "missing";
}

function describe(index: number, table: WasmTable): string {
	const max = table.max === null ? "no max" : `max=${table.max}`;

	return `table ${index} (${table.source} ${table.element} min=${table.min} ${max})`;
}

/** Checks that `bytes` exports a growable externref table as `__wbindgen_externrefs`. */
export function checkExternrefTable(bytes: Uint8Array): ExternrefTableCheck {
	const tables: WasmTable[] = [];
	let exportIndex: number | "missing" | "not-a-table" = "missing";

	try {
		const reader = new Reader(bytes);
		readHeader(reader);

		while (!reader.done) {
			const id = reader.byte();
			const section = reader.section(reader.unsigned());

			if (id === SECTION_IMPORT) readImports(section, tables);
			else if (id === SECTION_TABLE) readTables(section, tables);
			else if (id === SECTION_EXPORT) exportIndex = readTableExport(section, EXTERNREF_EXPORT);
		}
	} catch (error) {
		if (!(error instanceof WasmFormatError)) throw error;

		return { ok: false, message: `unreadable module: ${error.message}`, tables, exported: null };
	}

	if (exportIndex === "missing") {
		return { ok: false, message: `no "${EXTERNREF_EXPORT}" export`, tables, exported: null };
	}

	if (exportIndex === "not-a-table") {
		return {
			ok: false,
			message: `"${EXTERNREF_EXPORT}" is exported, but not as a table`,
			tables,
			exported: null,
		};
	}

	const table = tables[exportIndex];

	if (table === undefined) {
		return {
			ok: false,
			message: `"${EXTERNREF_EXPORT}" refers to table ${exportIndex}, but the module has ${tables.length} table(s)`,
			tables,
			exported: null,
		};
	}

	const exported = { index: exportIndex, table };
	const target = `"${EXTERNREF_EXPORT}" -> ${describe(exportIndex, table)}`;

	if (table.element !== "externref") {
		return {
			ok: false,
			message: `${target}: the element type must be externref`,
			tables,
			exported,
		};
	}

	if (table.max !== null && table.max - table.min < REQUIRED_GROWTH) {
		return {
			ok: false,
			message: `${target}: the glue grows it by ${REQUIRED_GROWTH}, which the maximum forbids`,
			tables,
			exported,
		};
	}

	return { ok: true, message: target, tables, exported };
}

function main(paths: string[]): number {
	if (paths.length === 0) {
		console.error("usage: node scripts/check-wasm-externref-table.ts <main_bg.wasm>...");

		return 2;
	}

	let failures = 0;

	for (const path of paths) {
		const result = checkExternrefTable(readFileSync(path));

		if (result.ok) {
			console.log(`ok   ${path}: ${result.message}`);
		} else {
			failures += 1;
			console.error(`FAIL ${path}: ${result.message}`);
			result.tables.forEach((table, index) => console.error(`     ${describe(index, table)}`));
		}
	}

	return failures === 0 ? 0 : 1;
}

// Run the CLI only when this file is the entry module, so tests can import
// the checker. The entry path is canonicalized because the ESM loader
// resolves import.meta.url through real paths (macOS /var → /private/var).
const entryPath = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined;

if (entryPath !== undefined && import.meta.url === entryPath) {
	process.exit(main(process.argv.slice(2)));
}
