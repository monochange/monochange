import { copyFile, mkdtemp, mkdir, readFile, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { describe, expect, it, vi } from "vitest";
import {
	allowedPath,
	collectChanges,
	safePath,
	runOnce,
	exportRepository,
	readAgentFiles,
	writeAgentFiles,
	validateConfig,
} from "../../../packages/monochange__feedback-worker/src/worker.ts";

describe("connected worker boundaries", () => {
	it.each([
		"../secret",
		"/etc/passwd",
		"src/../../secret",
		".env",
		".git/config",
		".github/workflows/ci.yml",
		"C:\\secret",
		"src/private.pem",
	])("rejects %s", (path) => {
		expect(safePath(path)).toBe(false);
	});
	it("directory scopes require a path boundary", () => {
		expect(allowedPath("src/main.js", ["src/**"])).toBe(true);
		expect(allowedPath("src-other/main.js", ["src/**"])).toBe(false);
	});
	it("collects only approved regular text changes and rejects symlinks", async () => {
		const root = await mkdtemp(join(tmpdir(), "monochange-worker-test-"));
		const fixtures = resolve("fixtures/tests/feedback-widget/worker");
		try {
			await mkdir(join(root, "src"));
			const original = await readFile(join(fixtures, "before/main.js"));
			await copyFile(join(fixtures, "after/main.js"), join(root, "src/main.js"));
			const changes = await collectChanges(
				root,
				new Map([["src/main.js", { content: original, executable: false }]]),
				["src/**"],
			);
			expect(changes).toEqual([
				{
					path: "src/main.js",
					executable: false,
					content: await readFile(join(fixtures, "after/main.js"), "utf8"),
				},
			]);
			await symlink(join(fixtures, "before/main.js"), join(root, "src/link.js"));
			await expect(collectChanges(root, new Map(), ["src/**"])).rejects.toThrow("symlink");
		} finally {
			await rm(root, { recursive: true, force: true });
		}
	});
	it("does not accept remote repositories or arbitrary command strings", () => {
		expect(() =>
			validateConfig({
				endpoint: "https://feedback.example",
				project: 1,
				repositories: { "acme/web": "https://example.com/repo" },
				model: {
					endpoint: "https://model.example/completions",
					name: "test",
					keyEnvironment: "MODEL_KEY",
				},
				container: { engine: "docker", image: "local-test", checks: [["node", "--test"]] },
			}),
		).toThrow("absolute paths");
	});
});

it("exports pinned Git blobs without credentials or secret files and preserves executable metadata", async () => {
	const repository = await mkdtemp(join(tmpdir(), "monochange-worker-git-"));
	const destination = await mkdtemp(join(tmpdir(), "monochange-worker-export-"));
	const content = await readFile(resolve("fixtures/tests/feedback-widget/worker/before/main.js"));
	const git = (args: string[], input?: Buffer | string) =>
		execFileSync("git", ["-C", repository, ...args], { input, encoding: "utf8" }).trim();
	try {
		git(["init", "--quiet"]);
		const blob = git(["hash-object", "-w", "--stdin"], content);
		// A tree is enough to exercise the export protocol; this fixture creates no commits.
		const tree = git(["mktree"], `100755 blob ${blob}\tmain.js\n100644 blob ${blob}\t.env\n`);
		const baseline = await exportRepository(repository, tree, destination);
		expect([...baseline.keys()]).toEqual(["main.js"]);
		expect(baseline.get("main.js")).toEqual({ content, executable: true });
		expect(await readAgentFiles(destination, ["main.js"])).toEqual([
			{ path: "main.js", content: content.toString("utf8") },
		]);
		const next = await readFile(
			resolve("fixtures/tests/feedback-widget/worker/after/main.js"),
			"utf8",
		);
		await writeAgentFiles(destination, ["main.js"], [{ path: "main.js", content: next }]);
		expect((await collectChanges(destination, baseline, ["main.js"]))[0].executable).toBe(true);
		await expect(
			writeAgentFiles(destination, ["main.js"], [{ path: "other.js", content: next }]),
		).rejects.toThrow("approved plan");
	} finally {
		await rm(repository, { recursive: true, force: true });
		await rm(destination, { recursive: true, force: true });
	}
});

it("reconciles a saved result before claiming work and preserves it when acknowledgement fails", async () => {
	const root = await mkdtemp(join(tmpdir(), "monochange-worker-reconnect-"));
	const spool = join(root, "pending.json");
	const config = {
		endpoint: "https://feedback.example",
		project: 1,
		workerToken: "test-only-worker",
	};
	try {
		await copyFile(resolve("fixtures/tests/feedback-widget/worker/pending.json"), spool);
		const original = await readFile(spool, "utf8");
		const fetcher = vi.fn(async (_url: string) => new Response("unavailable", { status: 503 }));
		vi.stubGlobal("fetch", fetcher);
		await expect(runOnce(config as any, spool)).rejects.toThrow("503");
		expect(await readFile(spool, "utf8")).toBe(original);
		fetcher.mockImplementation(async () => Response.json({ ok: true }));
		expect(await runOnce(config as any, spool)).toBe(true);
		expect(fetcher).toHaveBeenCalledTimes(2);
		expect(fetcher.mock.calls.every(([url]) => String(url).endsWith("/jobs/job-1/result"))).toBe(
			true,
		);
		await expect(readFile(spool)).rejects.toThrow();
	} finally {
		vi.unstubAllGlobals();
		await rm(root, { recursive: true, force: true });
	}
});
