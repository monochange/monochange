import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

const manifestPath = process.argv[2];
assert(manifestPath, "Usage: node scripts/check-app-release-preview.ts <manifest.json>");
const manifest: unknown = JSON.parse(readFileSync(manifestPath, "utf8"));
assert(isRecord(manifest) && manifest.dry_run === true, "Expected a dry-run release manifest");
assert(Array.isArray(manifest.changelogs), "Release manifest has no changelog artifact list");
assert(Array.isArray(manifest.changesets), "Release manifest has no changeset list");

const destinations = new Map([
	[
		"website_developer",
		{ stream: "default", format: "keep_a_changelog", path: /^app\/developer-changelog\.md$/ },
	],
	["website", { stream: "website", format: "keep_a_changelog", path: /^app\/changelog\.md$/ }],
	[
		"website_json",
		{ stream: "website", format: "json", path: /^app\/public\/releases\/[^/]+\.json$/ },
	],
]);
const appArtifacts = manifest.changelogs
	.map((artifact: unknown) => {
		assert(isRecord(artifact), "Expected a changelog artifact object");
		return artifact;
	})
	.filter((artifact) => artifact.owner_id === "monochange_app");
const hasAppNotes = manifest.changesets.some(
	(changeset: unknown) =>
		isRecord(changeset) &&
		Array.isArray(changeset.targets) &&
		changeset.targets.some((target: unknown) => isRecord(target) && target.id === "monochange_app"),
);
assert(
	!hasAppNotes || appArtifacts.length > 0,
	"App changesets produced no app release-note artifacts",
);

const seen = new Set<string>();
for (const artifact of appArtifacts) {
	const { output, stream, format, path, rendered } = artifact;
	assert.equal(artifact.owner_kind, "package");
	assert(
		typeof output === "string" && typeof stream === "string",
		"Expected a named app changelog output and stream",
	);
	const expected = destinations.get(output);
	assert(expected, `Unknown app changelog output: ${output}`);
	assert(!seen.has(output), `Duplicate app changelog output: ${output}`);
	assert.equal(stream, expected.stream, `Incorrect stream for ${output}`);
	assert.equal(format, expected.format, `Incorrect format for ${output}`);
	assert(typeof path === "string" && expected.path.test(path), `Incorrect path for ${output}`);
	assert(
		typeof rendered === "string" && rendered.trim().length > 0,
		`Empty app changelog output: ${output}`,
	);
	seen.add(output);
	console.log(`${output}\t${stream}\t${path}`);
}

if (seen.has("website") || seen.has("website_json")) {
	assert(
		seen.has("website") && seen.has("website_json"),
		"Website notes need both Markdown and JSON outputs",
	);
}
