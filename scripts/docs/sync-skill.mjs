#!/usr/bin/env node
// Sync the agent skill into the CLI crate so `monochange skill` can serve it.
//
// `packages/monochange__skill` is the source of truth: it is what npm publishes
// and what humans edit. The CLI cannot `include_str!` those files directly
// because `monochange` publishes to crates.io, and cargo only packages files
// inside the crate directory. So the skill is copied to `crates/monochange/skill/`
// and committed, and this script is the only sanctioned way to move bytes
// between the two.
//
//   node scripts/docs/sync-skill.mjs           copy source of truth into the crate
//   node scripts/docs/sync-skill.mjs --check   exit 1 when the copy has drifted
//
// `docs:check` runs the check so an edit to the published skill without a
// matching copy fails CI rather than shipping a stale CLI.

import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const source = path.join(root, "packages", "monochange__skill");
const destination = path.join(root, "crates", "monochange", "skill");

// Keep this list in lockstep with the `REFERENCES` table in
// `crates/monochange/src/skill.rs`: the CLI can only serve what is copied here.
const SKILL_FILES = [
	"adoption.md",
	"artifact-types.md",
	"change-classification.md",
	"changeset-guide.md",
	"changesets.md",
	"commands.md",
	"configuration.md",
	"linting.md",
	"multi-package-publishing.md",
	"readme.md",
	"reference.md",
	"trusted-publishing.md",
];
const EXAMPLE_FILES = [
	"migration.md",
	"publishing.md",
	"quickstart.md",
	"readme.md",
	"release-pr.md",
];

const FILES = [
	"SKILL.md",
	...SKILL_FILES.map((file) => `skills/${file}`),
	...EXAMPLE_FILES.map((file) => `examples/${file}`),
];

const check = process.argv.includes("--check");

if (!existsSync(source)) {
	console.error(`Skill source not found: ${source}`);
	process.exit(1);
}

const drift = [];

for (const file of FILES) {
	const from = path.join(source, file);
	const to = path.join(destination, file);
	if (!existsSync(from)) {
		console.error(`Source file missing: ${from}`);
		process.exit(1);
	}
	if (!check) {
		mkdirSync(path.dirname(to), { recursive: true });
		copyFileSync(from, to);
		continue;
	}
	if (!existsSync(to)) {
		drift.push(`${file} is missing from the CLI crate`);
		continue;
	}
	if (readFileSync(from, "utf8") !== readFileSync(to, "utf8")) {
		drift.push(`${file} differs from packages/monochange__skill`);
	}
}

if (check) {
	// A file removed from the source of truth must not linger in the crate, or
	// `monochange skill read` would serve content the package no longer
	// publishes.
	const stale = listFiles(destination)
		.map((served) => path.relative(destination, served).split(path.sep).join("/"))
		.filter((served) => !FILES.includes(served));
	for (const file of stale) {
		drift.push(`${file} is served by the CLI but absent from packages/monochange__skill`);
	}
}

if (drift.length > 0) {
	console.error("The skill copy in crates/monochange/skill has drifted:");
	for (const line of drift) {
		console.error(`  - ${line}`);
	}
	console.error("\nRun `node scripts/docs/sync-skill.mjs` and commit the result.");
	process.exit(1);
}

if (!check) {
	console.log(`Synced ${FILES.length} skill files into crates/monochange/skill.`);
}

function listFiles(directory) {
	if (!existsSync(directory)) {
		return [];
	}
	const found = [];
	for (const entry of readdirSync(directory, { withFileTypes: true })) {
		const child = path.join(directory, entry.name);
		if (entry.isDirectory()) {
			found.push(...listFiles(child));
		} else if (entry.isFile()) {
			found.push(child);
		}
	}
	return found;
}
