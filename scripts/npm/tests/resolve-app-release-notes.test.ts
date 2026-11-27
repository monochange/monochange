import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import {
	chmodSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { afterEach, describe, test } from "vitest";

import {
	ReleaseNotesResolutionError,
	compareSemver,
	parseStableSemver,
	readAppWorkspaceVersion,
	resolveAppReleaseNotes,
} from "../../resolve-app-release-notes.ts";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const resolverScript = join(repoRoot, "scripts", "resolve-app-release-notes.ts");
const verifyImageScript = join(repoRoot, "app", "deploy", "verify-image.sh");

const tempRoots: string[] = [];
const openServers: Server[] = [];

afterEach(async () => {
	for (const server of openServers.splice(0)) {
		await new Promise<void>((resolveClose) => {
			server.close(() => resolveClose());
		});
	}

	for (const root of tempRoots.splice(0)) {
		rmSync(root, { recursive: true, force: true });
	}
});

function makeTempDir(prefix: string): string {
	const dir = mkdtempSync(join(tmpdir(), prefix));
	tempRoots.push(dir);

	return dir;
}

const HTTP_OK = 200;
const HTTP_NOT_FOUND = 404;

function makeWebsiteReleaseJson(
	version: string,
	options: { title?: string; stream?: string } = {},
): string {
	const entry = {
		summary: `Release ${version}`,
		details_markdown: `User-facing notes for ${version}.`,
		change_type: "website_fix",
		bump: "patch",
		stream: options.stream ?? "website",
		style: "compact",
	};
	const payload = {
		title: options.title ?? version,
		summary: [],
		sections: [{ title: "Improved", collapsed: false, entries: [entry] }],
	};

	return `${JSON.stringify(payload, null, 2)}\n`;
}

function makeSimulatedRepo(options: {
	appVersion: string;
	releaseVersions: string[];
	changelogVersions?: string[];
}): string {
	const root = makeTempDir("app-release-notes-");
	const releasesDir = join(root, "app", "public", "releases");
	mkdirSync(releasesDir, { recursive: true });
	writeFileSync(
		join(root, "app", "Cargo.toml"),
		`[workspace]\nmembers = ["crates/*"]\n\n[workspace.package]\nversion = "${options.appVersion}"\n`,
	);

	for (const version of options.releaseVersions) {
		writeFileSync(join(releasesDir, `${version}.json`), makeWebsiteReleaseJson(version));
	}

	const headingVersions = options.changelogVersions ?? [...options.releaseVersions].reverse();
	const changelogSections = headingVersions
		.map((version) => `## ${version}\n\n### Improved\n\n- **Release ${version}.**`)
		.join("\n\n");
	writeFileSync(
		join(root, "app", "changelog.md"),
		`# What's new on monochange.dev\n\n${changelogSections}\n`,
	);

	return root;
}

function makeReleaseRecord(
	root: string,
	changelogs: { owner_id: string; output: string; stream: string; rendered: string }[],
	wrapped = true,
): string {
	const path = join(root, "release-record.json");
	const payload = wrapped ? { record: { changelogs } } : { changelogs };
	writeFileSync(path, `${JSON.stringify(payload, null, 2)}\n`);

	return path;
}

/** Minimal valid wasm module exporting a growable externref table at index 1. */
function makeValidExternrefWasm(): Uint8Array {
	const uleb = (value: number): number[] => {
		const out: number[] = [];
		let remaining = value;

		do {
			let byte = remaining & 0x7f;
			remaining = Math.floor(remaining / 128);
			if (remaining !== 0) byte |= 0x80;
			out.push(byte);
		} while (remaining !== 0);

		return out;
	};
	const encodeName = (text: string): number[] => {
		const bytes = [...Buffer.from(text, "utf8")];

		return [...uleb(bytes.length), ...bytes];
	};
	const vector = (items: number[][]): number[] => [...uleb(items.length), ...items.flat()];
	const section = (id: number, payload: number[]): number[] => [
		id,
		...uleb(payload.length),
		...payload,
	];

	const tableSection = section(
		4,
		vector([
			[0x70, 0x01, ...uleb(10), ...uleb(10)],
			[0x6f, 0x00, ...uleb(1024)],
		]),
	);
	const exportSection = section(
		7,
		vector([
			[...encodeName("__indirect_function_table"), 0x01, ...uleb(0)],
			[...encodeName("__wbindgen_externrefs"), 0x01, ...uleb(1)],
		]),
	);

	return new Uint8Array([
		0x00,
		0x61,
		0x73,
		0x6d,
		0x01,
		0x00,
		0x00,
		0x00,
		...tableSection,
		...exportSection,
	]);
}

describe("resolveAppReleaseNotes", () => {
	test("parses and compares strict stable SemVer", () => {
		assert.deepEqual(parseStableSemver("0.5.3"), [0, 5, 3]);
		assert.equal(compareSemver([0, 5, 2], [0, 5, 3]), -1);
		assert.equal(compareSemver([0, 10, 0], [0, 5, 3]), 1);
		assert.equal(compareSemver([0, 5, 3], [0, 5, 3]), 0);
		assert.throws(() => parseStableSemver("0.5.3-alpha.1"), ReleaseNotesResolutionError);
		assert.throws(() => parseStableSemver("01.2.3"), ReleaseNotesResolutionError);
	});

	test("resolves the current version for a website-stream release", () => {
		const root = makeSimulatedRepo({
			appVersion: "0.5.2",
			releaseVersions: ["0.5.0", "0.5.1", "0.5.2"],
		});
		const record = makeReleaseRecord(root, [
			{
				owner_id: "monochange_app",
				output: "website_developer",
				stream: "default",
				rendered: "## 0.5.2\n\nDeveloper note",
			},
			{
				owner_id: "monochange_app",
				output: "website",
				stream: "website",
				rendered: "## 0.5.2\n\nUser note",
			},
			{
				owner_id: "monochange_app",
				output: "website_json",
				stream: "website",
				rendered: makeWebsiteReleaseJson("0.5.2"),
			},
		]);

		const result = resolveAppReleaseNotes({
			repoRoot: root,
			appVersion: "0.5.2",
			releaseRecordPath: record,
		});
		assert.equal(result.appVersion, "0.5.2");
		assert.equal(result.notesVersion, "0.5.2");
		assert.equal(result.hasCurrentVersionNotes, true);
		assert.equal(result.releaseNotesSource, "website");
		assert.equal(result.releaseNotesMarkdown, "## 0.5.2\n\nUser note");
	});

	test("resolves the latest published website release for a developer-only release (PR #919 shape)", () => {
		const root = makeSimulatedRepo({
			appVersion: "0.5.3",
			releaseVersions: ["0.5.0", "0.5.1", "0.5.2"],
		});
		const record = makeReleaseRecord(
			root,
			[
				{
					owner_id: "monochange_app",
					output: "website_developer",
					stream: "default",
					rendered: "## 0.5.3\n\n### 🐛 Fixed\n\n- Docs alignment",
				},
			],
			false,
		);

		const result = resolveAppReleaseNotes({
			repoRoot: root,
			releaseRecordPath: record,
		});
		assert.equal(result.appVersion, "0.5.3");
		assert.equal(result.notesVersion, "0.5.2");
		assert.equal(result.hasCurrentVersionNotes, false);
		assert.equal(result.releaseNotesSource, "website_developer");
		assert.match(result.releaseNotesMarkdown ?? "", /Docs alignment/);
	});

	test("rejects a website-stream release whose public JSON file is missing", () => {
		const root = makeSimulatedRepo({
			appVersion: "0.5.3",
			releaseVersions: ["0.5.1", "0.5.2"],
		});
		const record = makeReleaseRecord(root, [
			{
				owner_id: "monochange_app",
				output: "website",
				stream: "website",
				rendered: "## 0.5.3\n\nUser note",
			},
			{
				owner_id: "monochange_app",
				output: "website_json",
				stream: "website",
				rendered: makeWebsiteReleaseJson("0.5.3"),
			},
		]);

		assert.throws(
			() =>
				resolveAppReleaseNotes({
					repoRoot: root,
					appVersion: "0.5.3",
					releaseRecordPath: record,
				}),
			/app\/public\/releases\/0\.5\.3\.json is missing/,
		);
	});

	test("rejects app/changelog.md and app/public/releases parity mismatches", () => {
		const missingJsonRoot = makeSimulatedRepo({
			appVersion: "0.5.3",
			releaseVersions: ["0.5.1", "0.5.2"],
			changelogVersions: ["0.5.3", "0.5.2", "0.5.1"],
		});
		assert.throws(
			() => resolveAppReleaseNotes({ repoRoot: missingJsonRoot }),
			/app\/changelog\.md has "## 0\.5\.3" without a matching app\/public\/releases\/0\.5\.3\.json/,
		);

		const missingHeadingRoot = makeSimulatedRepo({
			appVersion: "0.5.3",
			releaseVersions: ["0.5.1", "0.5.2", "0.5.3"],
			changelogVersions: ["0.5.2", "0.5.1"],
		});
		assert.throws(
			() => resolveAppReleaseNotes({ repoRoot: missingHeadingRoot }),
			/app\/public\/releases\/0\.5\.3\.json exists without a matching "## 0\.5\.3" section/,
		);
	});

	test("rejects release JSON ahead of app/Cargo.toml or with invalid stream/title", () => {
		const aheadRoot = makeSimulatedRepo({
			appVersion: "0.5.2",
			releaseVersions: ["0.5.2", "0.5.3"],
		});
		assert.throws(
			() => resolveAppReleaseNotes({ repoRoot: aheadRoot }),
			/ahead of app version \(0\.5\.2\)/,
		);

		const badStreamRoot = makeSimulatedRepo({
			appVersion: "0.5.2",
			releaseVersions: ["0.5.2"],
		});
		writeFileSync(
			join(badStreamRoot, "app", "public", "releases", "0.5.2.json"),
			makeWebsiteReleaseJson("0.5.2", { stream: "default" }),
		);
		assert.throws(
			() => resolveAppReleaseNotes({ repoRoot: badStreamRoot }),
			/every entry must belong to the "website" stream/,
		);

		const badTitleRoot = makeSimulatedRepo({
			appVersion: "0.5.2",
			releaseVersions: ["0.5.2"],
		});
		writeFileSync(
			join(badTitleRoot, "app", "public", "releases", "0.5.2.json"),
			makeWebsiteReleaseJson("0.5.2", { title: "0.5.1" }),
		);
		assert.throws(
			() => resolveAppReleaseNotes({ repoRoot: badTitleRoot }),
			/expected title "0\.5\.2", found "0\.5\.1"/,
		);
	});

	test("rejects partial website record outputs or unrecorded release JSON", () => {
		const root = makeSimulatedRepo({
			appVersion: "0.5.3",
			releaseVersions: ["0.5.2", "0.5.3"],
		});
		const partialRecord = makeReleaseRecord(root, [
			{
				owner_id: "monochange_app",
				output: "website",
				stream: "website",
				rendered: "## 0.5.3\n\nOnly markdown",
			},
		]);
		assert.throws(
			() => resolveAppReleaseNotes({ repoRoot: root, releaseRecordPath: partialRecord }),
			/must include both "website" and "website_json"/,
		);

		const devOnlyWithStrayJson = makeReleaseRecord(root, [
			{
				owner_id: "monochange_app",
				output: "website_developer",
				stream: "default",
				rendered: "## 0.5.3\n\nDev only",
			},
		]);
		assert.throws(
			() => resolveAppReleaseNotes({ repoRoot: root, releaseRecordPath: devOnlyWithStrayJson }),
			/exists, but the release record has no website_json output/,
		);
	});

	test("validates the live repository and a simulated developer-only bump", () => {
		const currentVersion = readAppWorkspaceVersion(repoRoot);
		const current = resolveAppReleaseNotes({ repoRoot, appVersion: currentVersion });
		assert.equal(current.appVersion, currentVersion);
		assert.ok(current.availableVersions.length > 0);

		const [major, minor, patch] = parseStableSemver(currentVersion);
		const simulatedNextPatch = `${major}.${minor}.${patch + 1}`;
		const simulated = resolveAppReleaseNotes({
			repoRoot,
			appVersion: simulatedNextPatch,
		});
		assert.equal(simulated.appVersion, simulatedNextPatch);
		assert.equal(simulated.notesVersion, current.notesVersion);
		assert.equal(simulated.hasCurrentVersionNotes, false);
	});
});

describe("CLI", () => {
	test("prints notes version and writes markdown notes with --write-notes", () => {
		const root = makeSimulatedRepo({
			appVersion: "0.5.3",
			releaseVersions: ["0.5.2"],
		});
		const record = makeReleaseRecord(root, [
			{
				owner_id: "monochange_app",
				output: "website_developer",
				stream: "default",
				rendered: "## 0.5.3\n\nOperator fix",
			},
		]);
		const notesOut = join(root, "out", "release-notes.md");

		const result = spawnSync(
			process.execPath,
			[
				resolverScript,
				"--root",
				root,
				"--app-version",
				"0.5.3",
				"--release-record",
				record,
				"--write-notes",
				notesOut,
			],
			{ encoding: "utf8" },
		);
		assert.equal(result.status, 0, result.stderr);
		assert.equal(result.stdout.trim(), "0.5.2");
		assert.equal(existsSync(notesOut), true);
		assert.equal(readFileSync(notesOut, "utf8"), "## 0.5.3\n\nOperator fix\n");
	});

	test("exits with code 2 when --write-notes is passed without --release-record", () => {
		const result = spawnSync(
			process.execPath,
			[resolverScript, "--write-notes", "/tmp/unused.md"],
			{ encoding: "utf8" },
		);
		assert.equal(result.status, 2);
		assert.match(result.stderr, /--write-notes requires --release-record/);
	});
});

describe("verify-image.sh end-to-end smoke check", () => {
	async function runVerifyImage(options: {
		repoAppVersion: string;
		repoReleaseVersions: string[];
		servedHealthVersion: string;
		servedReleaseVersions: string[];
		changelogHtml: string;
		explicitNotesVersion?: string;
	}) {
		const repo = makeSimulatedRepo({
			appVersion: options.repoAppVersion,
			releaseVersions: options.repoReleaseVersions,
		});
		const shimDir = makeTempDir("verify-image-shim-");
		const wasmPath = join(shimDir, "valid.wasm");
		writeFileSync(wasmPath, makeValidExternrefWasm());

		const dockerShim = join(shimDir, "docker");
		writeFileSync(
			dockerShim,
			`#!/usr/bin/env bash
set -euo pipefail
cmd="\${1:-}"
shift || true
case "$cmd" in
  run)
    echo "fake-container-id"
    ;;
  exec)
    shift # container name
    shift # sh
    shift # -c
    expr="$1"
    case "$expr" in
      *Uid*) echo "1000" ;;
      *NoNewPrivs*) echo "1" ;;
      *__wasm_split_manifest.json*) echo '{"route":["chunk_0"]}' ;;
      *) exit 1 ;;
    esac
    ;;
  cp)
    cp "${wasmPath}" "$2"
    ;;
  logs|rm)
    ;;
  *)
    echo "unexpected docker command: $cmd" >&2
    exit 1
    ;;
esac
`,
		);
		chmodSync(dockerShim, 0o755);

		const servedReleases = new Map<string, string>(
			options.servedReleaseVersions.map((version) => [version, makeWebsiteReleaseJson(version)]),
		);

		const server = createServer((req, res) => {
			const url = req.url ?? "/";

			if (url === "/health") {
				res.writeHead(HTTP_OK, { "content-type": "application/json" });
				res.end(
					JSON.stringify({
						status: "ok",
						version: options.servedHealthVersion,
						http: "up",
					}),
				);

				return;
			}

			if (url === "/changelog") {
				res.writeHead(HTTP_OK, { "content-type": "text/html; charset=utf-8" });
				res.end(options.changelogHtml);

				return;
			}

			const releaseMatch = /^\/releases\/([^/]+)\.json$/u.exec(url);

			if (releaseMatch !== null) {
				const body = servedReleases.get(releaseMatch[1]);

				if (body === undefined) {
					res.writeHead(HTTP_NOT_FOUND);
					res.end("not found");

					return;
				}

				res.writeHead(HTTP_OK, { "content-type": "application/json" });
				res.end(body);

				return;
			}

			const headers: Record<string, string> = { "content-type": "application/octet-stream" };
			const acceptEncoding = req.headers["accept-encoding"];

			if (
				typeof acceptEncoding === "string" &&
				(acceptEncoding === "gzip" || acceptEncoding === "br")
			) {
				headers["content-encoding"] = acceptEncoding;
			}

			res.writeHead(HTTP_OK, headers);
			res.end("ok");
		});
		openServers.push(server);

		await new Promise<void>((resolveListen) => {
			server.listen(0, "127.0.0.1", () => resolveListen());
		});
		const address = server.address() as AddressInfo;

		const args = [verifyImageScript, "monochange-app:smoke", options.repoAppVersion];
		if (options.explicitNotesVersion !== undefined) {
			args.push(options.explicitNotesVersion);
		}

		return await new Promise<{ status: number | null; stdout: string; stderr: string }>(
			(resolveRun) => {
				const child = spawn("bash", args, {
					env: {
						...process.env,
						PATH: `${shimDir}:${process.env.PATH ?? ""}:/usr/bin:/bin`,
						VERIFY_IMAGE_PORT: String(address.port),
						VERIFY_IMAGE_ROOT: repo,
					},
					stdio: ["ignore", "pipe", "pipe"],
				});
				let stdout = "";
				let stderr = "";
				child.stdout.on("data", (chunk: Buffer) => {
					stdout += chunk.toString("utf8");
				});
				child.stderr.on("data", (chunk: Buffer) => {
					stderr += chunk.toString("utf8");
				});
				child.once("close", (status) => {
					resolveRun({ status, stdout, stderr });
				});
			},
		);
	}

	test("passes for a developer-only release (0.5.3 image, latest website notes 0.5.2)", async () => {
		const result = await runVerifyImage({
			repoAppVersion: "0.5.3",
			repoReleaseVersions: ["0.5.1", "0.5.2"],
			servedHealthVersion: "0.5.3",
			servedReleaseVersions: ["0.5.1", "0.5.2"],
			changelogHtml:
				'<article id="v0.5.2"><h2>v0.5.2</h2><a href="/releases/0.5.2.json">View JSON</a></article><footer>Website v0.5.3</footer>',
		});
		assert.equal(result.status, 0, result.stderr);
	});

	test("passes for a website-stream release (0.5.3 image and 0.5.3 website notes)", async () => {
		const result = await runVerifyImage({
			repoAppVersion: "0.5.3",
			repoReleaseVersions: ["0.5.2", "0.5.3"],
			servedHealthVersion: "0.5.3",
			servedReleaseVersions: ["0.5.2", "0.5.3"],
			changelogHtml:
				'<article id="v0.5.3"><h2>v0.5.3</h2><a href="/releases/0.5.3.json">View JSON</a></article><footer>Website v0.5.3</footer>',
		});
		assert.equal(result.status, 0, result.stderr);
	});

	test("fails when a website-stream release image omits its own release JSON", async () => {
		const result = await runVerifyImage({
			repoAppVersion: "0.5.3",
			repoReleaseVersions: ["0.5.2", "0.5.3"],
			servedHealthVersion: "0.5.3",
			servedReleaseVersions: ["0.5.2"],
			changelogHtml:
				'<article id="v0.5.2"><h2>v0.5.2</h2><a href="/releases/0.5.2.json">View JSON</a></article><footer>Website v0.5.3</footer>',
		});
		assert.equal(result.status, 22, result.stderr);
		assert.match(result.stdout, /ok\s+.*site\.wasm/);
		assert.match(result.stderr, /404/);
	});

	test("fails when /health reports a stale version", async () => {
		const result = await runVerifyImage({
			repoAppVersion: "0.5.3",
			repoReleaseVersions: ["0.5.2"],
			servedHealthVersion: "0.5.2",
			servedReleaseVersions: ["0.5.2"],
			changelogHtml:
				'<article id="v0.5.2"><h2>v0.5.2</h2><a href="/releases/0.5.2.json">View JSON</a></article><footer>Website v0.5.2</footer>',
		});
		assert.equal(result.status, 1, result.stderr);
		assert.equal(result.stderr, "");
		assert.match(result.stdout, /\bfalse\b/);
	});

	test("fails when /changelog omits the release card link even if the footer shows the version", async () => {
		const result = await runVerifyImage({
			repoAppVersion: "0.5.3",
			repoReleaseVersions: ["0.5.2", "0.5.3"],
			servedHealthVersion: "0.5.3",
			servedReleaseVersions: ["0.5.2", "0.5.3"],
			changelogHtml: '<div class="release-empty">Empty</div><footer>Website v0.5.3</footer>',
		});
		assert.equal(result.status, 1, result.stderr);
		assert.equal(result.stderr, "");
		assert.match(result.stdout, /ok\s+.*site\.wasm/);
	});
});
