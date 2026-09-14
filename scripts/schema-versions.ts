#!/usr/bin/env node

import { readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = join(fileURLToPath(import.meta.url), "..");
const repoRoot = resolve(__dirname, "..");

const SCHEMA_URL_BASE = "https://monochange.github.io/monochange/schemas";

/**
 * Contract versions are `major.minor`, and the breaking axis shifts at 1.0:
 * while the major is 0 every minor may break consumers, and from 1.0 onward
 * only a major bump may. Stable assets worth publishing a link for are
 * therefore every `0.N` and only `N.0`.
 */
export function isBreakingAxisVersion(version) {
	const [major, minor] = version.split(".");
	return major === "0" || minor === "0";
}

function compareVersions(left, right) {
	const [leftMajor, leftMinor] = left.split(".").map(Number);
	const [rightMajor, rightMinor] = right.split(".").map(Number);
	return leftMajor - rightMajor || leftMinor - rightMinor;
}

/**
 * Read the committed versioned schema assets and group them by family.
 *
 * The committed files are the published artifacts, so they are a truer source
 * than git tags: not every schema family is tagged, and CI checks out shallowly
 * where tags are absent entirely.
 */
export function collectSchemaVersions(schemasDir) {
	const families = new Map();

	for (const entry of readdirSync(schemasDir)) {
		const match = /^(?<family>.+?)\.v(?<version>\d+\.\d+)\.schema\.json$/.exec(entry);
		if (!match?.groups) {
			continue;
		}
		const { family, version } = match.groups;
		if (!isBreakingAxisVersion(version)) {
			continue;
		}
		const versions = families.get(family) ?? new Set();
		versions.add(version);
		families.set(family, versions);
	}

	return [...families.entries()]
		.map(([family, versions]) => ({
			family,
			versions: [...versions].toSorted(compareVersions),
		}))
		.toSorted((left, right) => left.family.localeCompare(right.family));
}

/**
 * Render the schema index as markdown.
 *
 * Returns markdown rather than JSON because mdt injects this through a bare
 * `{{ value }}` in a provider block. A `{% for %}` loop would be reflowed by
 * dprint, which treats jinja control tags as ordinary prose and can merge them
 * onto adjacent lines; markdown list items survive formatting untouched.
 *
 * The result intentionally has no trailing newline. mdt appends the block's own
 * boundary newlines, and a trailing newline here would leave one that `dprint`
 * strips, so `mdt check` would fail after every format pass.
 */
export function renderSchemaIndex(families, { urlBase = SCHEMA_URL_BASE } = {}) {
	const lines = [];
	for (const { family, versions } of families) {
		lines.push(`**\`${family}.schema.json\`**`, "");
		lines.push(`- Current: <${urlBase}/${family}.schema.json>`);
		for (const version of versions) {
			lines.push(`- v${version}: <${urlBase}/${family}.v${version}.schema.json>`);
		}
		lines.push("");
	}
	return lines.join("\n").replace(/\n+$/, "");
}

function main() {
	const schemasDir = join(repoRoot, "docs/src/schemas");
	const families = collectSchemaVersions(schemasDir);
	process.stdout.write(renderSchemaIndex(families));
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
	main();
}
