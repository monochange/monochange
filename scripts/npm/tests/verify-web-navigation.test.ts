import assert from "node:assert/strict";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { describe, test } from "vitest";

import {
	executeVerification,
	freePort,
	type FixtureIdentity,
	type VerificationConfig,
	OwnedProcesses,
	projectsSignedInFailures,
	signedOutControlsFailures,
	startOwnedServer,
	websiteSignedInFailures,
} from "../../verify-web-navigation.ts";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const fixturesRoot = join(repoRoot, "fixtures/tests/verify-web-navigation");
const htmlDir = join(fixturesRoot, "html");
const helpersDir = join(fixturesRoot, "helpers");

// The identities are baked into the committed fixture HTML; the tests assert
// with exactly these values.
const fixtureIdentity: FixtureIdentity = {
	userId: 1,
	githubId: 424_242,
	login: "fixture-alice-4b81c2",
	orgGithubId: 515_151,
	orgLogin: "fixture-org-9a7c31",
	installationGithubId: 616_161,
	repoGithubId: 717_171,
	repoName: "fixture-repo-77c4d1",
	projectSlug: "fixture-project-3e2f90",
	projectName: "fixture-project-3e2f90",
};

function readHtml(name: string): string {
	return readFileSync(join(htmlDir, name), "utf8");
}

function readPid(pidFile: string): number {
	return Number.parseInt(readFileSync(pidFile, "utf8"), 10);
}

/** Observable reaping: the pid the helper recorded is no longer signalable. */
async function waitForPidExit(pid: number, timeoutMs = 5000): Promise<boolean> {
	const deadline = Date.now() + timeoutMs;

	while (Date.now() < deadline) {
		try {
			process.kill(pid, 0);
		} catch {
			return true;
		}

		await new Promise((resolve) => {
			setTimeout(resolve, 50);
		});
	}

	return false;
}

const passingRoutes = {
	"/": {
		signedIn: "website-signed-in.html",
		anonymous: "website-anonymous.html",
		invalid: "website-invalid-cookie.html",
	},
	"/projects": {
		signedIn: "projects-signed-in.html",
		anonymous: "projects-anonymous.html",
		invalid: "projects-anonymous.html",
	},
};

type JourneyResult = { code: number; outLines: string[]; errLines: string[] };

/** Runs the harness's server-side journey against the fixture helper. */
async function runJourney(options: {
	routes: Record<string, Record<string, string>>;
	helper?: string;
	workDir: string;
	pidFile: string;
}): Promise<JourneyResult> {
	const outLines: string[] = [];
	const errLines: string[] = [];
	const config: VerificationConfig = {
		workDir: options.workDir,
		databaseFile: join(options.workDir, "verify.sqlite3"),
		jwtSecret: "verify-web-navigation-test-secret",
		identity: fixtureIdentity,
		startupTimeoutMs: 8000,
		settleMs: 10,
		out: (line) => {
			outLines.push(line);
		},
		err: (line) => {
			errLines.push(line);
		},
		server: {
			command: process.execPath,
			args: [join(helpersDir, options.helper ?? "serve-fixture.mjs")],
			cwd: helpersDir,
			env: {
				...process.env,
				VERIFY_HTML_DIR: htmlDir,
				VERIFY_LOGIN: fixtureIdentity.login,
				VERIFY_PIDFILE: options.pidFile,
				VERIFY_ROUTES: JSON.stringify(options.routes),
			},
		},
	};
	const code = await executeVerification(config);

	return { code, outLines, errLines };
}

// ── Raw SSR identity assertions ─────────────────────────────────────────────

describe("raw SSR identity assertions", () => {
	test("the exact seeded identity satisfies the website SSR assertion", () => {
		assert.deepEqual(
			websiteSignedInFailures(readHtml("website-signed-in.html"), fixtureIdentity.login),
			[],
		);
	});

	test("the exact seeded identity satisfies the projects SSR assertion", () => {
		assert.deepEqual(
			projectsSignedInFailures(
				readHtml("projects-signed-in.html"),
				fixtureIdentity.orgLogin,
				fixtureIdentity.projectName,
			),
			[],
		);
	});

	test("a generic Sign out marker without raw identity fails the website assertion", () => {
		// The fixture carries a Sign out button; the identity only appears in
		// a script that adds it after hydration.
		const failures = websiteSignedInFailures(
			readHtml("website-hydrated-only.html"),
			fixtureIdentity.login,
		);

		assert.equal(failures.length, 1);
		assert.match(failures[0] ?? "", /do not contain the signed-in fixture identity/);
		assert.match(failures[0] ?? "", /post-hydration DOM is not accepted/);
	});

	test("a generic Sign out marker without raw identity fails the projects assertion", () => {
		const failures = projectsSignedInFailures(
			readHtml("projects-hydrated-only.html"),
			fixtureIdentity.orgLogin,
			fixtureIdentity.projectName,
		);

		assert.equal(failures.length, 2);
	});

	test("the wrong fixture identity fails the website assertion", () => {
		const failures = websiteSignedInFailures(
			readHtml("website-wrong-identity.html"),
			fixtureIdentity.login,
		);

		assert.equal(failures.length, 1);
	});

	test("the wrong fixture identity fails the projects assertion", () => {
		const failures = projectsSignedInFailures(
			readHtml("projects-wrong-identity.html"),
			fixtureIdentity.orgLogin,
			fixtureIdentity.projectName,
		);

		assert.equal(failures.length, 2);
	});

	test("the anonymous website document cannot satisfy the signed-in assertion", () => {
		assert.equal(
			websiteSignedInFailures(readHtml("website-anonymous.html"), fixtureIdentity.login).length,
			1,
		);
	});

	test("the anonymous projects document cannot satisfy the signed-in assertion", () => {
		assert.equal(
			projectsSignedInFailures(
				readHtml("projects-anonymous.html"),
				fixtureIdentity.orgLogin,
				fixtureIdentity.projectName,
			).length,
			2,
		);
	});

	test("anonymous and invalid-cookie controls carry no signed-in identity", () => {
		const failures = signedOutControlsFailures(
			{
				anonymousWebsite: readHtml("website-anonymous.html"),
				anonymousProjects: readHtml("projects-anonymous.html"),
				invalidWebsite: readHtml("website-invalid-cookie.html"),
				invalidProjects: readHtml("projects-anonymous.html"),
			},
			fixtureIdentity,
		);

		assert.deepEqual(failures, []);
	});

	test("a control response that leaked signed-in identity fails", () => {
		const signedInWebsite = readHtml("website-signed-in.html");
		const failures = signedOutControlsFailures(
			{
				anonymousWebsite: signedInWebsite,
				anonymousProjects: readHtml("projects-anonymous.html"),
				invalidWebsite: readHtml("website-invalid-cookie.html"),
				invalidProjects: readHtml("projects-anonymous.html"),
			},
			fixtureIdentity,
		);

		assert.equal(failures.length, 1);
		assert.match(failures[0] ?? "", /anonymousWebsite control response contains/);
	});
});

// ── Startup waits racing process exit and deadline ──────────────────────────

describe("owned server startup", () => {
	test("resolves when the owned process becomes healthy and reaps it afterwards", async () => {
		const registry = new OwnedProcesses();
		const dir = mkdtempSync(join(tmpdir(), "verify-web-nav-startup-"));
		const pidFile = join(dir, "server.pid");

		try {
			const server = await startOwnedServer({
				registry,
				command: process.execPath,
				args: [join(helpersDir, "serve-fixture.mjs")],
				cwd: helpersDir,
				env: { ...process.env, VERIFY_PIDFILE: pidFile, VERIFY_ROUTES: "{}" },
				port: await freePort(),
				timeoutMs: 8000,
			});
			const response = await fetch(`${server.baseUrl}/health`);

			assert.equal(response.status, 200);
		} finally {
			const errors = await registry.shutdown();

			assert.deepEqual(errors, []);
			assert.equal(await waitForPidExit(readPid(pidFile)), true);
			rmSync(dir, { recursive: true, force: true });
		}
	}, 20_000);

	test("reports partial startup failure when the process exits first", async () => {
		const registry = new OwnedProcesses();
		const dir = mkdtempSync(join(tmpdir(), "verify-web-nav-startup-"));
		const pidFile = join(dir, "server.pid");

		try {
			await assert.rejects(
				startOwnedServer({
					registry,
					command: process.execPath,
					args: [join(helpersDir, "fail-fast.mjs")],
					cwd: helpersDir,
					env: { ...process.env, VERIFY_PIDFILE: pidFile },
					port: await freePort(),
					timeoutMs: 8000,
				}),
				/ended before becoming healthy.*exit code 3/s,
			);
		} finally {
			const errors = await registry.shutdown();

			assert.deepEqual(errors, []);
			assert.equal(await waitForPidExit(readPid(pidFile)), true);
			rmSync(dir, { recursive: true, force: true });
		}
	}, 20_000);

	test("times out when the process never becomes healthy and still reaps it", async () => {
		const registry = new OwnedProcesses();
		const dir = mkdtempSync(join(tmpdir(), "verify-web-nav-startup-"));
		const pidFile = join(dir, "server.pid");

		try {
			await assert.rejects(
				startOwnedServer({
					registry,
					command: process.execPath,
					args: [join(helpersDir, "never-healthy.mjs")],
					cwd: helpersDir,
					env: { ...process.env, VERIFY_PIDFILE: pidFile },
					port: await freePort(),
					timeoutMs: 1200,
				}),
				/did not become healthy within 1200 ms/,
			);
		} finally {
			const errors = await registry.shutdown();

			assert.deepEqual(errors, []);
			assert.equal(await waitForPidExit(readPid(pidFile)), true);
			rmSync(dir, { recursive: true, force: true });
		}
	}, 20_000);

	test("never accepts a stale responder on a reused port and leaves it untouched", async () => {
		const port = await freePort();
		const stale = http.createServer((request, response) => {
			response.writeHead(200, { "content-type": "application/json" });
			response.end('{"status":"ok","responder":"stale"}');
		});

		await new Promise<void>((resolve) => {
			stale.listen(port, "127.0.0.1", resolve);
		});

		const registry = new OwnedProcesses();
		const dir = mkdtempSync(join(tmpdir(), "verify-web-nav-startup-"));
		const pidFile = join(dir, "server.pid");

		try {
			await assert.rejects(
				startOwnedServer({
					registry,
					command: process.execPath,
					args: [join(helpersDir, "fail-on-port.mjs")],
					cwd: helpersDir,
					env: { ...process.env, VERIFY_PIDFILE: pidFile },
					port,
					timeoutMs: 8000,
				}),
				/refusing to verify against a process this run does not own/,
			);
		} finally {
			const errors = await registry.shutdown();

			assert.deepEqual(errors, []);

			// The responder this run does not own must still be alive:
			// cleanup may only ever signal its own children.
			const response = await fetch(`http://127.0.0.1:${port}/health`);
			const body = await response.text();

			assert.equal(response.status, 200);
			assert.match(body, /"responder":"stale"/);

			await new Promise<void>((resolve) => {
				stale.close(() => {
					resolve();
				});
			});
			assert.equal(await waitForPidExit(readPid(pidFile)), true);
			rmSync(dir, { recursive: true, force: true });
		}
	}, 20_000);
});

// ── Server-side journey through the real orchestrator ───────────────────────

describe("verification journey (server-side phases, no browser)", () => {
	test("a passing journey prints success only after cleanup removed the work dir", async () => {
		const dir = mkdtempSync(join(tmpdir(), "verify-web-nav-journey-"));
		const workDir = join(dir, "work");

		mkdirSync(workDir);
		try {
			const result = await runJourney({
				routes: passingRoutes,
				workDir,
				pidFile: join(dir, "server.pid"),
			});

			assert.equal(result.code, 0, result.errLines.join("\n"));
			assert.match(
				result.outLines.at(-1) ?? "",
				/server-side verification passed/,
				"success must be the final claim",
			);
			assert.ok(
				result.outLines.some((line) => line.includes("raw SSR identity (direct HTTP")),
				"the raw SSR section names its transport",
			);
			assert.ok(result.outLines.some((line) => line.includes("browser navigation: skipped")));
			assert.equal(existsSync(workDir), false, "cleanup must remove the work dir");
			assert.equal(await waitForPidExit(readPid(join(dir, "server.pid"))), true);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	}, 30_000);

	test("a wrong identity fails the journey without any success claim", async () => {
		const dir = mkdtempSync(join(tmpdir(), "verify-web-nav-journey-"));
		const workDir = join(dir, "work");

		mkdirSync(workDir);
		try {
			const result = await runJourney({
				routes: {
					"/": { ...passingRoutes["/"], signedIn: "website-wrong-identity.html" },
					"/projects": {
						...passingRoutes["/projects"],
						signedIn: "projects-wrong-identity.html",
					},
				},
				workDir,
				pidFile: join(dir, "server.pid"),
			});

			assert.equal(result.code, 1);
			assert.ok(
				result.errLines.some((line) => line.includes("verification failure")),
				"the nonzero exit must be explained",
			);
			assert.ok(
				!result.outLines.some((line) => line.includes("verification passed")),
				"no success may be printed for a failed journey",
			);
			assert.equal(existsSync(workDir), false, "cleanup still removes the work dir");
			assert.equal(await waitForPidExit(readPid(join(dir, "server.pid"))), true);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	}, 30_000);

	test("a partial startup failure exits nonzero with the helpers reaped", async () => {
		const dir = mkdtempSync(join(tmpdir(), "verify-web-nav-journey-"));
		const workDir = join(dir, "work");

		mkdirSync(workDir);
		try {
			const result = await runJourney({
				routes: {},
				helper: "fail-fast.mjs",
				workDir,
				pidFile: join(dir, "server.pid"),
			});

			assert.equal(result.code, 1);
			assert.ok(result.errLines.some((line) => line.includes("ended before becoming healthy")));
			assert.ok(
				!result.outLines.some((line) => line.includes("verification passed")),
				"no success may be printed when startup failed",
			);
			assert.equal(existsSync(workDir), false, "cleanup still removes the work dir");
			assert.equal(await waitForPidExit(readPid(join(dir, "server.pid"))), true);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	}, 30_000);

	test.skipIf(process.platform === "win32")(
		"a cleanup failure exits nonzero, claims nothing, and still reaps the helpers",
		async () => {
			const dir = mkdtempSync(join(tmpdir(), "verify-web-nav-journey-"));
			const workDir = join(dir, "work");
			const locked = join(workDir, "locked");

			mkdirSync(workDir);
			mkdirSync(locked);
			chmodSync(locked, 0o000);

			try {
				const result = await runJourney({
					routes: passingRoutes,
					workDir,
					pidFile: join(dir, "server.pid"),
				});

				assert.equal(result.code, 1);
				assert.ok(
					result.errLines.some(
						(line) =>
							line.includes("cleanup FAIL") &&
							line.includes("could not remove the temporary work dir"),
					),
					"the cleanup error must be visible",
				);
				assert.ok(
					result.outLines.some((line) => line.startsWith("  ok: ")),
					"the checks themselves passed",
				);
				assert.ok(
					!result.outLines.some((line) => line.includes("verification passed")),
					"a cleanup failure may not print success",
				);
				assert.equal(existsSync(workDir), true, "the undeletable work dir remains");
				assert.equal(
					await waitForPidExit(readPid(join(dir, "server.pid"))),
					true,
					"owned processes are reaped even when dir removal fails",
				);
			} finally {
				chmodSync(locked, 0o755);
				rmSync(dir, { recursive: true, force: true });
			}
		},
		30_000,
	);
});
