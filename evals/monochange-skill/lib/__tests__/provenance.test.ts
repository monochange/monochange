import assert from "node:assert/strict";
import { join } from "node:path";
import { describe, test } from "vitest";

import { readProvenance } from "../provenance.ts";

const fixtures = join(import.meta.dirname, "fixtures", "provenance");

describe("saved evaluation provenance", () => {
	test("installed agent runs retain the original checkout, CLI, and skill hashes", () => {
		const provenance = readProvenance(join(fixtures, "installed.json"), true);
		assert.equal(provenance.checkoutCommit, "3621d40db11bd2dfd89192bf7270b4a50603293a");
		assert.equal(
			provenance.cliSha256,
			"8c1de8264dc2124deba1a163e2852e2a663a2c4603e55d2b451dc27339f20eae",
		);
		assert.equal(
			provenance.skillSha256,
			"85175ae1715bbc37ecebbedc88b686cad07483bf72de1e08b884320fe851b833",
		);
	});

	test("CLI discovery and agent-free contracts require no installed skill hash", () => {
		const provenance = readProvenance(join(fixtures, "cli.json"), false);
		assert.equal(provenance.skillSha256, undefined);
		assert.equal(provenance.checkoutCommit, "3621d40db11bd2dfd89192bf7270b4a50603293a");
	});

	test("an installed-agent replay cannot omit its evaluated skill hash", () => {
		assert.throws(() => readProvenance(join(fixtures, "cli.json"), true), /skill provenance/);
	});

	test.each(["empty", "null", "missing-cli", "invalid-checkout", "invalid-cli", "invalid-skill"])(
		"incomplete or malformed %s metadata cannot certify a replay",
		(fixture) => {
			assert.throws(
				() => readProvenance(join(fixtures, `${fixture}.json`), false),
				/Invalid saved evaluation/,
			);
		},
	);
});
