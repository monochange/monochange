import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, test } from "vitest";
import {
	collectSchemaVersions,
	isBreakingAxisVersion,
	renderSchemaIndex,
} from "../../schema-versions.ts";

function makeSchemasDir(entries: string[]) {
	const dir = mkdtempSync(join(tmpdir(), "schema-versions-"));
	mkdirSync(dir, { recursive: true });
	for (const entry of entries) {
		writeFileSync(join(dir, entry), "{}\n");
	}
	return dir;
}

describe("isBreakingAxisVersion", () => {
	test("keeps every pre-1.0 minor because each may break consumers", () => {
		assert.equal(isBreakingAxisVersion("0.1"), true);
		assert.equal(isBreakingAxisVersion("0.2"), true);
		assert.equal(isBreakingAxisVersion("0.9"), true);
	});

	test("keeps only the .0 major release from 1.0 onward", () => {
		assert.equal(isBreakingAxisVersion("1.0"), true);
		assert.equal(isBreakingAxisVersion("2.0"), true);
		assert.equal(isBreakingAxisVersion("1.1"), false);
		assert.equal(isBreakingAxisVersion("2.3"), false);
	});
});

describe("collectSchemaVersions", () => {
	test("groups versioned assets by family and ignores moving aliases", () => {
		const dir = makeSchemasDir([
			"demo.schema.json",
			"demo.v0.1.schema.json",
			"demo.v0.2.schema.json",
			"other.v1.0.schema.json",
			"other.v1.1.schema.json",
		]);

		assert.deepEqual(collectSchemaVersions(dir), [
			{ family: "demo", versions: ["0.1", "0.2"] },
			{ family: "other", versions: ["1.0"] },
		]);
	});

	test("sorts versions numerically rather than lexically", () => {
		const dir = makeSchemasDir([
			"demo.v0.10.schema.json",
			"demo.v0.2.schema.json",
			"demo.v0.9.schema.json",
		]);

		const [family] = collectSchemaVersions(dir);
		assert.deepEqual(family?.versions, ["0.2", "0.9", "0.10"]);
	});

	test("ignores files that are not versioned schema assets", () => {
		const dir = makeSchemasDir(["readme.md", "demo.v0.1.schema.json.bak", "demo.schema.json"]);

		assert.deepEqual(collectSchemaVersions(dir), []);
	});
});

describe("renderSchemaIndex", () => {
	test("lists the moving alias once per family and every kept version", () => {
		const markdown = renderSchemaIndex([{ family: "demo", versions: ["0.1", "1.0"] }]);

		assert.equal(
			markdown,
			[
				"**`demo.schema.json`**",
				"",
				"- Current: <https://monochange.github.io/monochange/schemas/demo.schema.json>",
				"- v0.1: <https://monochange.github.io/monochange/schemas/demo.v0.1.schema.json>",
				"- v1.0: <https://monochange.github.io/monochange/schemas/demo.v1.0.schema.json>",
			].join("\n"),
		);
	});

	test("does not end with a newline", () => {
		// mdt appends the block's own boundary newlines; a trailing newline here
		// would leave one that dprint strips, so `mdt check` would fail after
		// every format pass.
		const markdown = renderSchemaIndex([{ family: "demo", versions: ["0.1"] }]);
		assert.equal(markdown.endsWith("\n"), false);
	});

	test("separates families with a blank line", () => {
		const markdown = renderSchemaIndex([
			{ family: "a", versions: ["0.1"] },
			{ family: "b", versions: ["0.1"] },
		]);
		assert.ok(markdown.includes(".schema.json>\n\n**`b.schema.json`**"));
	});
});
