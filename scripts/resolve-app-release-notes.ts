#!/usr/bin/env node
// Resolves and validates the website-stream release-notes version that a built
// `monochange_app` image must serve at `/releases/<version>.json` and render on
// `/changelog`.
//
// `monochange_app` uses two changelog streams in `monochange.toml`:
// - `default` (`website_developer` -> `app/developer-changelog.md`) for
//   operator, CI, docs, and internal maintenance releases
// - `website` (`website` -> `app/changelog.md`, `website_json` ->
//   `app/public/releases/{{ version }}.json`) for user-facing website updates
//
// Developer-only releases bump `app/Cargo.toml` without adding a new website
// JSON file; `/health` reports the new app version while `/releases/` and
// `/changelog` serve the latest published website release. Website-stream
// releases must ship and serve their exact `<version>.json` and Markdown entry.
import {
	existsSync,
	mkdirSync,
	readFileSync,
	readdirSync,
	realpathSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { basename, dirname, extname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const STABLE_SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/u;
const CHANGELOG_HEADING = /^##\s+([^\s]+)\s*$/u;
const WORKSPACE_VERSION_LINE = /^version\s*=\s*"([^"]+)"\s*$/u;
const EXIT_OK = 0;
const EXIT_FAILURE = 1;
const EXIT_USAGE = 2;

export class ReleaseNotesResolutionError extends Error {}

export type SemverTuple = readonly [number, number, number];

export type ReleaseNotesSource = "website" | "website_developer" | null;

export type ResolvedAppReleaseNotes = {
	appVersion: string;
	notesVersion: string;
	hasCurrentVersionNotes: boolean;
	availableVersions: string[];
	releaseNotesSource: ReleaseNotesSource;
	releaseNotesMarkdown: string | null;
};

type JsonRecord = Record<string, unknown>;

type ReleaseRecordChangelog = {
	output: string;
	stream: string;
	rendered: string;
};

type CliOptions = {
	repoRoot: string;
	appVersion?: string;
	releaseRecordPath?: string;
	writeNotesPath?: string;
	format: "text" | "json";
};

function isRecord(value: unknown): value is JsonRecord {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function fail(message: string): never {
	throw new ReleaseNotesResolutionError(message);
}

/** Parses a strict stable `major.minor.patch` version (no pre-release or build metadata). */
export function parseStableSemver(value: string, label = "version"): SemverTuple {
	const match = STABLE_SEMVER.exec(value.trim());

	if (match === null) {
		fail(`invalid stable SemVer ${label}: ${JSON.stringify(value)}`);
	}

	return [Number(match[1]), Number(match[2]), Number(match[3])];
}

/** Compares two strict stable SemVer tuples (`-1`, `0`, `1`). */
export function compareSemver(left: SemverTuple, right: SemverTuple): number {
	for (let index = 0; index < 3; index += 1) {
		if (left[index] !== right[index]) {
			return left[index] < right[index] ? -1 : 1;
		}
	}

	return 0;
}

/** Reads `[workspace.package] version` from `app/Cargo.toml`. */
export function readAppWorkspaceVersion(repoRoot: string): string {
	const manifestPath = join(repoRoot, "app", "Cargo.toml");

	if (!existsSync(manifestPath)) {
		fail(`missing app workspace manifest: ${manifestPath}`);
	}

	const lines = readFileSync(manifestPath, "utf8").split(/\r?\n/u);
	let inWorkspacePackage = false;

	for (const line of lines) {
		const trimmed = line.trim();

		if (trimmed.startsWith("[")) {
			inWorkspacePackage = trimmed === "[workspace.package]";
			continue;
		}

		if (!inWorkspacePackage) {
			continue;
		}

		const match = WORKSPACE_VERSION_LINE.exec(trimmed);

		if (match !== null) {
			parseStableSemver(match[1], "app/Cargo.toml version");

			return match[1];
		}
	}

	fail(`could not find [workspace.package] version in ${manifestPath}`);
}

function parseJsonFile(filePath: string, label: string): JsonRecord {
	let parsed: unknown;

	try {
		parsed = JSON.parse(readFileSync(filePath, "utf8"));
	} catch (error) {
		const reason = error instanceof Error ? error.message : String(error);
		fail(`invalid JSON in ${label} ${filePath}: ${reason}`);
	}

	if (!isRecord(parsed)) {
		fail(`${filePath}: expected a JSON object`);
	}

	return parsed;
}

function validateReleaseSection(filePath: string, section: unknown): void {
	if (!isRecord(section) || !Array.isArray(section.entries)) {
		fail(`${filePath}: each section must have an entries array`);
	}

	for (const entry of section.entries) {
		if (!isRecord(entry) || entry.stream !== "website") {
			fail(`${filePath}: every entry must belong to the "website" stream`);
		}
	}
}

function validateReleaseJsonFile(filePath: string, expectedVersion: string): void {
	const parsed = parseJsonFile(filePath, "release notes");

	if (parsed.title !== expectedVersion) {
		fail(
			`${filePath}: expected title ${JSON.stringify(expectedVersion)}, found ${JSON.stringify(parsed.title)}`,
		);
	}

	if (!Array.isArray(parsed.sections)) {
		fail(`${filePath}: expected sections array`);
	}

	for (const section of parsed.sections) {
		validateReleaseSection(filePath, section);
	}
}

function readPublicReleaseVersions(repoRoot: string): string[] {
	const releasesDir = join(repoRoot, "app", "public", "releases");

	if (!existsSync(releasesDir) || !statSync(releasesDir).isDirectory()) {
		fail(`missing website releases directory: ${releasesDir}`);
	}

	const entries: { version: string; parsed: SemverTuple }[] = [];

	for (const name of readdirSync(releasesDir)) {
		const fullPath = join(releasesDir, name);

		if (!statSync(fullPath).isFile() || extname(name) !== ".json") {
			continue;
		}

		const stem = basename(name, ".json");
		const parsed = parseStableSemver(stem, `release filename ${name}`);
		validateReleaseJsonFile(fullPath, stem);
		entries.push({ version: stem, parsed });
	}

	if (entries.length === 0) {
		fail(`no website release JSON files found in ${releasesDir}`);
	}

	entries.sort((left, right) => compareSemver(right.parsed, left.parsed));

	return entries.map((entry) => entry.version);
}

function readWebsiteChangelogVersions(repoRoot: string): string[] {
	const changelogPath = join(repoRoot, "app", "changelog.md");

	if (!existsSync(changelogPath) || !statSync(changelogPath).isFile()) {
		fail(`missing website changelog: ${changelogPath}`);
	}

	const versions: string[] = [];
	const seen = new Set<string>();

	for (const line of readFileSync(changelogPath, "utf8").split(/\r?\n/u)) {
		const match = CHANGELOG_HEADING.exec(line);

		if (match === null) {
			continue;
		}

		const version = match[1];
		parseStableSemver(version, `app/changelog.md heading "${line}"`);

		if (seen.has(version)) {
			fail(`duplicate release heading ${version} in ${changelogPath}`);
		}

		seen.add(version);
		versions.push(version);
	}

	if (versions.length === 0) {
		fail(`no release headings found in ${changelogPath}`);
	}

	return versions;
}

function validateChangelogParity(
	appVersion: string,
	availableVersions: string[],
	changelogVersions: string[],
): void {
	const availableSet = new Set(availableVersions);
	const changelogSet = new Set(changelogVersions);

	for (const version of availableVersions) {
		if (!changelogSet.has(version)) {
			fail(
				`app/public/releases/${version}.json exists without a matching "## ${version}" section in app/changelog.md`,
			);
		}
	}

	for (const version of changelogVersions) {
		if (!availableSet.has(version)) {
			fail(
				`app/changelog.md has "## ${version}" without a matching app/public/releases/${version}.json`,
			);
		}
	}

	const latestNotesVersion = availableVersions[0];

	if (compareSemver(parseStableSemver(latestNotesVersion), parseStableSemver(appVersion)) > 0) {
		fail(
			`latest website release notes (${latestNotesVersion}) are ahead of app version (${appVersion})`,
		);
	}

	if (changelogVersions[0] !== latestNotesVersion) {
		fail(
			`top heading in app/changelog.md (${changelogVersions[0]}) does not match latest release JSON (${latestNotesVersion})`,
		);
	}
}

function readAppRecordChangelogs(recordPath: string): ReleaseRecordChangelog[] {
	if (!existsSync(recordPath)) {
		fail(`missing release record file: ${recordPath}`);
	}

	const parsed = parseJsonFile(recordPath, "release record");

	// Support both `monochange step release-record --format json` ({ record: { changelogs } })
	// and raw `.monochange/releases/<hash>/release.json` ({ changelogs }).
	const container = isRecord(parsed.record) ? parsed.record : parsed;

	if (!Array.isArray(container.changelogs)) {
		fail(`${recordPath}: missing changelogs array`);
	}

	const results: ReleaseRecordChangelog[] = [];

	for (const entry of container.changelogs) {
		if (!isRecord(entry) || entry.owner_id !== "monochange_app") {
			continue;
		}

		if (
			typeof entry.output !== "string" ||
			typeof entry.stream !== "string" ||
			typeof entry.rendered !== "string"
		) {
			fail(`${recordPath}: malformed monochange_app changelog entry`);
		}

		results.push({
			output: entry.output,
			stream: entry.stream,
			rendered: entry.rendered,
		});
	}

	return results;
}

function resolveRecordNotes(
	recordPath: string,
	appVersion: string,
	hasCurrentVersionNotes: boolean,
): { source: "website" | "website_developer"; markdown: string } {
	const appChangelogs = readAppRecordChangelogs(recordPath);
	const websiteMd = appChangelogs.find(
		(entry) => entry.output === "website" && entry.stream === "website",
	);
	const websiteJson = appChangelogs.find(
		(entry) => entry.output === "website_json" && entry.stream === "website",
	);
	const websiteDeveloper = appChangelogs.find(
		(entry) => entry.output === "website_developer" && entry.stream === "default",
	);

	if (Boolean(websiteMd) !== Boolean(websiteJson)) {
		fail(
			`release record for monochange_app ${appVersion} must include both "website" and "website_json" outputs together`,
		);
	}

	if (websiteMd !== undefined && websiteJson !== undefined) {
		if (!hasCurrentVersionNotes) {
			fail(
				`release record includes website_json for ${appVersion}, but app/public/releases/${appVersion}.json is missing`,
			);
		}

		if (websiteMd.rendered.trim().length === 0) {
			fail(`release record "website" notes for ${appVersion} are empty`);
		}

		return { source: "website", markdown: websiteMd.rendered };
	}

	if (websiteDeveloper === undefined) {
		fail(
			`release record for monochange_app ${appVersion} has neither website-stream nor website_developer notes`,
		);
	}

	if (hasCurrentVersionNotes) {
		fail(
			`app/public/releases/${appVersion}.json exists, but the release record has no website_json output for ${appVersion}`,
		);
	}

	if (websiteDeveloper.rendered.trim().length === 0) {
		fail(`release record "website_developer" notes for ${appVersion} are empty`);
	}

	return { source: "website_developer", markdown: websiteDeveloper.rendered };
}

/**
 * Validates website release-note artifacts in `repoRoot` against `appVersion`
 * (and an optional release record) and returns the version that `/releases/`
 * and `/changelog` must serve.
 */
export function resolveAppReleaseNotes(options: {
	repoRoot: string;
	appVersion?: string;
	releaseRecordPath?: string;
}): ResolvedAppReleaseNotes {
	const repoRoot = resolve(options.repoRoot);
	const appVersion = options.appVersion ?? readAppWorkspaceVersion(repoRoot);
	parseStableSemver(appVersion, "appVersion");

	const availableVersions = readPublicReleaseVersions(repoRoot);
	const changelogVersions = readWebsiteChangelogVersions(repoRoot);
	validateChangelogParity(appVersion, availableVersions, changelogVersions);

	const hasCurrentVersionNotes = availableVersions.includes(appVersion);
	const recordNotes =
		options.releaseRecordPath === undefined
			? null
			: resolveRecordNotes(options.releaseRecordPath, appVersion, hasCurrentVersionNotes);

	return {
		appVersion,
		notesVersion: availableVersions[0],
		hasCurrentVersionNotes,
		availableVersions,
		releaseNotesSource: recordNotes?.source ?? null,
		releaseNotesMarkdown: recordNotes?.markdown ?? null,
	};
}

function defaultRepoRoot(): string {
	return resolve(dirname(fileURLToPath(import.meta.url)), "..");
}

function parseCliOptions(argv: string[]): CliOptions | { error: string } {
	let parsed: ReturnType<typeof parseArgs>;

	try {
		parsed = parseArgs({
			args: argv,
			options: {
				root: { type: "string" },
				"app-version": { type: "string" },
				"release-record": { type: "string" },
				"write-notes": { type: "string" },
				format: { type: "string", default: "text" },
			},
			allowPositionals: false,
			strict: true,
		});
	} catch (error) {
		return { error: error instanceof Error ? error.message : String(error) };
	}

	const format = parsed.values.format;

	if (format !== "text" && format !== "json") {
		return { error: `--format must be "text" or "json", got ${JSON.stringify(format)}` };
	}

	const writeNotesPath =
		typeof parsed.values["write-notes"] === "string" ? parsed.values["write-notes"] : undefined;
	const releaseRecordPath =
		typeof parsed.values["release-record"] === "string"
			? parsed.values["release-record"]
			: undefined;

	if (writeNotesPath !== undefined && releaseRecordPath === undefined) {
		return { error: "--write-notes requires --release-record" };
	}

	const repoRoot = typeof parsed.values.root === "string" ? parsed.values.root : defaultRepoRoot();
	const appVersion =
		typeof parsed.values["app-version"] === "string" ? parsed.values["app-version"] : undefined;

	return { repoRoot, appVersion, releaseRecordPath, writeNotesPath, format };
}

function writeReleaseNotesFile(targetPath: string, markdown: string | null): void {
	if (markdown === null) {
		fail("no release notes markdown available to write");
	}

	mkdirSync(dirname(resolve(targetPath)), { recursive: true });
	const content = markdown.endsWith("\n") ? markdown : `${markdown}\n`;
	writeFileSync(targetPath, content, "utf8");
}

function main(argv: string[]): number {
	const options = parseCliOptions(argv);

	if ("error" in options) {
		console.error(`usage error: ${options.error}`);

		return EXIT_USAGE;
	}

	try {
		const result = resolveAppReleaseNotes(options);

		if (options.writeNotesPath !== undefined) {
			writeReleaseNotesFile(options.writeNotesPath, result.releaseNotesMarkdown);
		}

		console.log(options.format === "json" ? JSON.stringify(result, null, 2) : result.notesVersion);

		return EXIT_OK;
	} catch (error) {
		const message = error instanceof Error ? error.message : String(error);
		console.error(`FAIL: ${message}`);

		return EXIT_FAILURE;
	}
}

const entryPath = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined;

if (entryPath !== undefined && import.meta.url === entryPath) {
	process.exit(main(process.argv.slice(2)));
}
