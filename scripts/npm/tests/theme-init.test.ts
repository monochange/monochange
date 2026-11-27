import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import { test } from "vitest";

const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const script = readFileSync(join(root, "app/crates/monochange_app/src/theme-init.js"), "utf8");

function initialClass(
	saved: string | null,
	systemDark: boolean,
	storageUnavailable = false,
): string {
	const document = { documentElement: { className: "" } };
	runInNewContext(script, {
		document,
		localStorage: {
			getItem: () => {
				if (storageUnavailable) throw new Error("storage unavailable");
				return saved;
			},
		},
		window: { matchMedia: () => ({ matches: systemDark }) },
	});
	return document.documentElement.className;
}

test("saved theme takes precedence over the system preference", () => {
	assert.equal(initialClass("dark", false), "dark");
	assert.equal(initialClass("light", true), "light");
	assert.equal(initialClass("invalid", true), "light");
});

test("the system preference is used when storage is empty or unavailable", () => {
	assert.equal(initialClass(null, true), "dark");
	assert.equal(initialClass(null, false), "light");
	assert.equal(initialClass(null, true, true), "dark");
});

test("the documented CSP hash matches the inlined script", () => {
	const hash = createHash("sha256").update(script).digest("base64");
	const docs = readFileSync(join(root, "docs/agents/tooling.md"), "utf8");
	assert.ok(docs.includes(`'sha256-${hash}'`));
});
