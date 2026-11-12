#!/usr/bin/env node
// End-to-end verification that the two browser frontends behave as one site
// with one session, split into two separately proven transports.
//
// 1. Raw SSR identity: the harness mints a session cookie and sends direct
//    HTTP requests to the production backend, asserting that the initial
//    response bytes for BOTH the website and the projects frontend contain
//    the unique seeded fixture identity. No browser JavaScript, no
//    post-hydration DOM, and no generic signed-in marker can satisfy these
//    assertions; anonymous and invalid-cookie controls must not carry the
//    identity either.
// 2. Browser navigation: a throwaway headless Chrome with its own
//    user-data-dir talks CDP over WebSocket (Node built-ins only, no
//    preview tools, no personal browser profile). Each navigation phase
//    tracks its own requests, so earlier phases cannot satisfy later
//    checks: the correct entry module loads per document, cross-frontend
//    links perform real document navigations with cookie identity, lazy
//    navigation stays client-side with its own split chunk, and no console,
//    hydration, MIME, or first-party failed-request errors occur.
//
// Process ownership is explicit: every spawned process is registered
// immediately, startup waits race process exit against the deadline (a
// stale responder on a reused port is never accepted), cleanup waits for
// owned shutdown before removing the temporary profile, and success is only
// reported after cleanup succeeded. Cleanup failures exit nonzero with no
// success claim.
//
// Usage:
//   pnpm node scripts/verify-web-navigation.ts \
//     [--site-root app/target/site] [--server-bin app/target/release/monochange_app]
//
// CHROME_PATH selects the Chrome/Chromium binary; the default looks in the
// usual install locations.
//
// The session token is minted here with the same HS256 claims schema as
// `monochange_app_api::create_token` (sub, github_id, github_login, exp,
// iat, epoch) using a per-run dummy secret this script hands the server; if
// that schema drifts, the signed-in assertions fail loudly rather than
// silently passing. The seeded GitHub identities are throwaway fixtures —
// no real accounts, no production secrets, no network calls beyond
// localhost (the fixture data is shaped so the surfaces never call the
// GitHub API).
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { createHmac, randomBytes } from "node:crypto";
import { existsSync, mkdtempSync, realpathSync, rmSync } from "node:fs";
import net from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..");
const SESSION_COOKIE = "__Host-monochange_session";

// ── Shared types ────────────────────────────────────────────────────────────

/** The throwaway identity one verification run seeds and asserts on. */
export type FixtureIdentity = {
	userId: number;
	githubId: number;
	login: string;
	orgGithubId: number;
	orgLogin: string;
	installationGithubId: number;
	repoGithubId: number;
	repoName: string;
	projectSlug: string;
	projectName: string;
};

/** A process this harness spawned and therefore owns the lifecycle of. */
export type ManagedProcess = {
	name: string;
	child: ChildProcess;
	/** Resolves when the child exits or fails to spawn; never rejects. */
	end: Promise<ChildEnd>;
	/** Resolves when the child exited AND its stdio streams drained. */
	closed: Promise<void>;
	isEnded(): boolean;
};

export type ChildEnd =
	| { kind: "exit"; code: number | null; signal: NodeJS.Signals | null }
	| { kind: "error"; error: Error };

export type ServerSpec = {
	command: string;
	args: string[];
	cwd: string;
	env: NodeJS.ProcessEnv;
};

export type VerificationConfig = {
	workDir: string;
	databaseFile: string;
	server: ServerSpec;
	jwtSecret: string;
	identity: FixtureIdentity;
	/** Runs after the server is healthy, so its migrations created the schema. */
	seedDatabase?: (databaseFile: string) => void;
	/** Without a Chrome command only the server-side phases run. */
	chrome?: { command: string };
	startupTimeoutMs?: number;
	chromeTimeoutMs?: number;
	settleMs?: number;
	out?: (line: string) => void;
	err?: (line: string) => void;
};

const DEFAULT_STARTUP_TIMEOUT_MS = 30_000;
const DEFAULT_CHROME_TIMEOUT_MS = 20_000;
const DEFAULT_SETTLE_MS = 1000;
const GRACEFUL_MS = 4000;
const KILL_MS = 2000;
const STREAM_MS = 2000;

// ── Small utilities ─────────────────────────────────────────────────────────

function sleep(ms: number): Promise<void> {
	return new Promise((resolveSleep) => {
		setTimeout(resolveSleep, ms);
	});
}

async function withTimeout<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
	let timer: ReturnType<typeof setTimeout> | undefined;

	try {
		return await Promise.race([
			promise,
			new Promise<never>((_, reject) => {
				timer = setTimeout(() => reject(new Error(message)), ms);
			}),
		]);
	} finally {
		if (timer !== undefined) clearTimeout(timer);
	}
}

export function freePort(): Promise<number> {
	return new Promise<number>((resolvePort, rejectPort) => {
		const server = net.createServer();
		server.unref();
		server.on("error", rejectPort);
		server.listen(0, "127.0.0.1", () => {
			const address = server.address();
			if (typeof address === "object" && address !== null) {
				const port = address.port;
				server.close(() => {
					resolvePort(port);
				});
			} else {
				rejectPort(new Error("could not bind a local port"));
			}
		});
	});
}

function describeEnd(end: ChildEnd): string {
	if (end.kind === "error") return `spawn failed: ${end.error.message}`;

	const detail = end.code !== null ? `exit code ${end.code}` : `signal ${end.signal ?? "unknown"}`;
	return detail;
}

/** Keeps the tail of a child's output for failure diagnostics. */
function createOutputRing(capacity = 4000) {
	let text = "";

	return {
		push(chunk: string): void {
			text = `${text}${chunk}`.slice(-capacity);
		},
		read(): string {
			return text;
		},
	};
}

function attachOutputRing(child: ChildProcess): ReturnType<typeof createOutputRing> {
	const ring = createOutputRing();
	child.stdout?.on("data", (chunk: Buffer) => {
		ring.push(chunk.toString());
	});
	child.stderr?.on("data", (chunk: Buffer) => {
		ring.push(chunk.toString());
	});

	return ring;
}

// ── Check reporting ─────────────────────────────────────────────────────────

class CheckLog {
	readonly failures: string[] = [];
	private readonly out: (line: string) => void;
	private readonly err: (line: string) => void;

	constructor(out: (line: string) => void, err: (line: string) => void) {
		this.out = out;
		this.err = err;
	}

	pass(message: string): void {
		this.out(`  ok: ${message}`);
	}

	fail(message: string): void {
		this.failures.push(message);
		this.err(`  FAIL: ${message}`);
	}

	check(condition: boolean, message: string): void {
		if (condition) {
			this.pass(message);
		} else {
			this.fail(message);
		}
	}
}

// ── Owned process registry ──────────────────────────────────────────────────

/**
 * Every process this harness spawns is registered here immediately, and only
 * processes in this registry are ever signalled — never a pid guessed from a
 * port, a broad process match, or a personal browser profile.
 */
export class OwnedProcesses {
	private readonly owned: ManagedProcess[] = [];

	register(name: string, child: ChildProcess): ManagedProcess {
		const state = { ended: false };
		const managed: ManagedProcess = {
			name,
			child,
			end: new Promise<ChildEnd>((resolveEnd) => {
				child.once("exit", (code, signal) => {
					state.ended = true;
					resolveEnd({ kind: "exit", code, signal });
				});
				child.once("error", (error) => {
					state.ended = true;
					resolveEnd({ kind: "error", error });
				});
			}),
			closed: new Promise<void>((resolveClosed) => {
				// Resolve without a value: the close event passes the exit
				// code as the first argument, and `null` (a signal death)
				// would collide with the timeout sentinel used in shutdown.
				child.once("close", () => resolveClosed());
			}),
			isEnded: () => state.ended,
		};

		this.owned.push(managed);

		return managed;
	}

	/**
	 * Stops every owned process in reverse startup order: a graceful SIGTERM,
	 * then a bounded SIGKILL for just that child, then confirmation that its
	 * stdio streams closed. Returns the cleanup errors, if any.
	 */
	async shutdown(): Promise<string[]> {
		const errors: string[] = [];

		for (const managed of this.owned.toReversed()) {
			if (!managed.isEnded()) {
				managed.child.kill("SIGTERM");
				const graceful = await Promise.race([managed.end, sleep(GRACEFUL_MS).then(() => null)]);

				if (graceful === null) {
					managed.child.kill("SIGKILL");
					const killed = await Promise.race([managed.end, sleep(KILL_MS).then(() => null)]);

					if (killed === null) {
						errors.push(
							`owned ${managed.name} did not exit after SIGKILL; leaving it alone rather than signalling anything wider`,
						);
						continue;
					}
				}
			}

			const streamsClosed = await Promise.race([managed.closed, sleep(STREAM_MS).then(() => null)]);

			if (streamsClosed === null) {
				errors.push(`owned ${managed.name} exited but its stdio streams did not close`);
			}
		}

		this.owned.length = 0;

		return errors;
	}
}

// ── Startup waits that race process exit against the deadline ───────────────

async function waitForHealthy(options: {
	managed: ManagedProcess;
	name: string;
	baseUrl: string;
	healthPath: string;
	timeoutMs: number;
	intervalMs: number;
	confirmationMs: number;
	outputTail: () => string;
}): Promise<void> {
	const { managed, name, baseUrl, healthPath, timeoutMs, intervalMs, confirmationMs, outputTail } =
		options;
	const deadline = Date.now() + timeoutMs;
	let lastError = "no attempt recorded";

	const fetchHealth = async (): Promise<"ok" | "unhealthy" | "unreachable"> => {
		try {
			const response = await fetch(`${baseUrl}${healthPath}`, {
				signal: AbortSignal.timeout(intervalMs * 10),
			});

			if (response.ok) return "ok";

			lastError = `${healthPath} answered ${response.status}`;

			return "unhealthy";
		} catch (error) {
			if (error instanceof Error && error.name !== "TimeoutError") {
				lastError = error.message;
			}

			return "unreachable";
		}
	};

	for (;;) {
		if (managed.isEnded()) {
			const end = await managed.end;

			throw new Error(
				`the ${name} process ended before becoming healthy (${describeEnd(end)}); output tail:\n${outputTail()}`,
			);
		}

		if ((await fetchHealth()) === "ok") {
			// A health answer is only proof when OUR process is the
			// responder. On a reused port a stale process answers while the
			// child we spawned is still booting towards its own bind
			// failure, so the answer must still be ours after a
			// confirmation window and a second fetch.
			await sleep(confirmationMs);

			if (managed.isEnded()) {
				const end = await managed.end;

				throw new Error(
					`a stale responder answered ${healthPath} while the ${name} process ended (${describeEnd(end)}); refusing to verify against a process this run does not own`,
				);
			}

			if ((await fetchHealth()) === "ok" && !managed.isEnded()) {
				return;
			}
		}

		if (Date.now() > deadline) {
			throw new Error(
				`the ${name} process did not become healthy within ${timeoutMs} ms (${lastError}); output tail:\n${outputTail()}`,
			);
		}

		await sleep(intervalMs);
	}
}

/**
 * Spawns the server as an owned process and waits for its health endpoint,
 * racing the child's exit against the deadline. The port is always set
 * explicitly so an ambient shell `PORT` can never point the verification at a
 * process this run does not own.
 */
export async function startOwnedServer(options: {
	registry: OwnedProcesses;
	command: string;
	args: string[];
	cwd: string;
	env: NodeJS.ProcessEnv;
	port: number;
	healthPath?: string;
	timeoutMs: number;
	intervalMs?: number;
	confirmationMs?: number;
}): Promise<{ baseUrl: string; outputTail: () => string }> {
	const child = spawn(options.command, options.args, {
		cwd: options.cwd,
		env: { ...options.env, PORT: String(options.port) },
		stdio: ["ignore", "pipe", "pipe"],
	});
	const managed = options.registry.register("server", child);
	const outputTail = attachOutputRing(child).read;
	const baseUrl = `http://127.0.0.1:${options.port}`;

	await waitForHealthy({
		managed,
		name: "server",
		baseUrl,
		healthPath: options.healthPath ?? "/health",
		timeoutMs: options.timeoutMs,
		intervalMs: options.intervalMs ?? 200,
		confirmationMs: options.confirmationMs ?? 750,
		outputTail,
	});

	return { baseUrl, outputTail };
}

// ── Session token (HS256, claims mirror monochange_app_api::create_token) ──

function base64url(input: string): string {
	return Buffer.from(input).toString("base64url");
}

export function mintSessionToken(secret: string, identity: FixtureIdentity): string {
	const now = Math.floor(Date.now() / 1000);
	const header = base64url(JSON.stringify({ alg: "HS256", typ: "JWT" }));
	const claims = base64url(
		JSON.stringify({
			sub: identity.userId,
			github_id: identity.githubId,
			github_login: identity.login,
			exp: now + 7 * 24 * 60 * 60,
			iat: now,
			epoch: 0,
		}),
	);
	const signingInput = `${header}.${claims}`;
	const signature = createHmac("sha256", secret).update(signingInput).digest("base64url");

	return `${signingInput}.${signature}`;
}

// ── Raw SSR identity assertions (pure, fixture-testable) ────────────────────

/**
 * The identity markers are the run-unique seeded strings themselves: the login
 * exists only in the seeded `users` row and the organization login only in the
 * seeded installation row, so their presence in the raw response bytes proves
 * the backend read the session during SSR. A generic marker such as a
 * rendered "Sign out" button appears for session states that carry no
 * identity at all and is deliberately never consulted here; identity a
 * browser would add after hydration is absent from these bytes by
 * construction.
 */
export function websiteSignedInFailures(html: string, login: string): string[] {
	const failures: string[] = [];

	if (!html.includes(login)) {
		failures.push(
			`the website SSR response bytes do not contain the signed-in fixture identity (${login}); a generic Sign out marker or post-hydration DOM is not accepted as SSR proof`,
		);
	}

	return failures;
}

export function projectsSignedInFailures(
	html: string,
	orgLogin: string,
	projectName: string,
): string[] {
	const failures: string[] = [];

	if (!html.includes(orgLogin)) {
		failures.push(
			`the projects SSR response bytes do not contain the seeded organization identity (${orgLogin}); a generic Sign out marker or post-hydration DOM is not accepted as SSR proof`,
		);
	}

	if (!html.includes(projectName)) {
		failures.push(
			`the projects SSR response bytes do not contain the seeded project (${projectName})`,
		);
	}

	return failures;
}

export type SignedOutControlCaptures = {
	anonymousWebsite: string;
	anonymousProjects: string;
	invalidWebsite: string;
	invalidProjects: string;
};

/**
 * Anonymous and invalid-cookie responses must not carry the signed-in
 * identity: a control that leaked it would make the signed-in assertions
 * meaningless.
 */
export function signedOutControlsFailures(
	captures: SignedOutControlCaptures,
	identity: FixtureIdentity,
): string[] {
	const failures: string[] = [];
	const markers = [identity.login, identity.orgLogin, identity.projectName];

	for (const [name, body] of Object.entries(captures)) {
		const leaked = markers.filter((marker) => body.includes(marker));

		if (leaked.length > 0) {
			failures.push(
				`the ${name} control response contains signed-in identity markers (${leaked.join(", ")})`,
			);
		}
	}

	return failures;
}

type SsrCapture = {
	status: number;
	contentType: string;
	body: string;
};

/** Extracts the actual initial response bytes over direct HTTP. */
async function captureSsr(
	baseUrl: string,
	path: string,
	cookie: string | undefined,
): Promise<SsrCapture> {
	const response = await fetch(`${baseUrl}${path}`, {
		headers: cookie === undefined ? {} : { cookie: `${SESSION_COOKIE}=${cookie}` },
		redirect: "manual",
		signal: AbortSignal.timeout(20_000),
	});

	return {
		status: response.status,
		contentType: response.headers.get("content-type") ?? "",
		body: await response.text(),
	};
}

async function runRawSsrPhase(options: {
	baseUrl: string;
	token: string;
	identity: FixtureIdentity;
	log: CheckLog;
	out: (line: string) => void;
}): Promise<void> {
	const { baseUrl, token, identity, log, out } = options;

	out("");
	out("raw SSR identity (direct HTTP request with the minted cookie; no browser, no hydration):");
	const website = await captureSsr(baseUrl, "/", token);
	const projects = await captureSsr(baseUrl, "/projects", token);

	log.check(
		website.status === 200 && website.contentType.includes("text/html"),
		`the website SSR document answers 200 text/html (${website.status} ${website.contentType})`,
	);
	log.check(
		projects.status === 200 && projects.contentType.includes("text/html"),
		`the projects SSR document answers 200 text/html (${projects.status} ${projects.contentType})`,
	);

	const websiteFailures = websiteSignedInFailures(website.body, identity.login);

	if (websiteFailures.length === 0) {
		log.pass(`the website SSR bytes contain the signed-in fixture identity (${identity.login})`);
	} else {
		websiteFailures.forEach((failure) => {
			log.fail(failure);
		});
	}

	const projectsFailures = projectsSignedInFailures(
		projects.body,
		identity.orgLogin,
		identity.projectName,
	);

	if (projectsFailures.length === 0) {
		log.pass(
			`the projects SSR bytes contain the seeded organization and project identity (${identity.orgLogin} / ${identity.projectName})`,
		);
	} else {
		projectsFailures.forEach((failure) => {
			log.fail(failure);
		});
	}

	out("  controls (anonymous and invalid cookie must not satisfy signed-in identity):");
	const controls = {
		anonymousWebsite: await captureSsr(baseUrl, "/", undefined),
		anonymousProjects: await captureSsr(baseUrl, "/projects", undefined),
		invalidWebsite: await captureSsr(baseUrl, "/", "not-a-jwt"),
		invalidProjects: await captureSsr(baseUrl, "/projects", "not-a-jwt"),
	};

	for (const [name, capture] of Object.entries(controls)) {
		log.check(
			capture.status === 200 && capture.contentType.includes("text/html"),
			`the ${name} control document answers 200 text/html (${capture.status} ${capture.contentType})`,
		);
	}

	const controlFailures = signedOutControlsFailures(
		{
			anonymousWebsite: controls.anonymousWebsite.body,
			anonymousProjects: controls.anonymousProjects.body,
			invalidWebsite: controls.invalidWebsite.body,
			invalidProjects: controls.invalidProjects.body,
		},
		identity,
	);

	if (controlFailures.length === 0) {
		log.pass("no anonymous or invalid-cookie control response contains the signed-in identity");
	} else {
		controlFailures.forEach((failure) => {
			log.fail(failure);
		});
	}
}

// ── Chrome + CDP ────────────────────────────────────────────────────────────

function findChrome(): string {
	const override = process.env.CHROME_PATH;
	const candidates = override
		? [override]
		: [
				"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
				"/usr/bin/google-chrome",
				"/usr/bin/chromium",
				"/usr/bin/chromium-browser",
			];
	const found = candidates.find((candidate) => existsSync(candidate));

	if (found === undefined) {
		throw new Error("no Chrome/Chromium found; set CHROME_PATH to the browser binary");
	}

	return found;
}

async function startOwnedChrome(options: {
	registry: OwnedProcesses;
	command: string;
	workDir: string;
	port: number;
	timeoutMs: number;
}): Promise<string> {
	const child = spawn(
		options.command,
		[
			"--headless=new",
			"--no-sandbox",
			"--disable-gpu",
			"--no-first-run",
			"--disable-extensions",
			`--remote-debugging-port=${options.port}`,
			`--user-data-dir=${join(options.workDir, "chrome-profile")}`,
			"about:blank",
		],
		{ stdio: ["ignore", "ignore", "pipe"] },
	);
	const managed = options.registry.register("chrome", child);
	const outputTail = attachOutputRing(child).read;
	const deadline = Date.now() + options.timeoutMs;

	while (Date.now() < deadline) {
		if (managed.isEnded()) {
			const end = await managed.end;

			throw new Error(
				`Chrome ended before its DevTools endpoint came up (${describeEnd(end)}); output tail:\n${outputTail()}`,
			);
		}

		try {
			const response = await fetch(`http://127.0.0.1:${options.port}/json/list`, {
				signal: AbortSignal.timeout(2000),
			});
			const targets = (await response.json()) as Array<{
				type?: string;
				webSocketDebuggerUrl?: string;
			}>;
			const page = targets.find((target) => target.type === "page");

			if (page?.webSocketDebuggerUrl !== undefined && !managed.isEnded()) {
				return page.webSocketDebuggerUrl;
			}
		} catch {
			// Chrome is still starting.
		}

		await sleep(200);
	}

	throw new Error(`Chrome's DevTools endpoint never came up within ${options.timeoutMs} ms`);
}

class Cdp {
	private nextId = 1;
	private readonly handlers = new Map<string, Array<(params: Record<string, unknown>) => void>>();
	private readonly pending = new Map<
		number,
		{ resolve: (value: Record<string, unknown>) => void; reject: (error: Error) => void }
	>();

	private readonly ws: WebSocket;

	constructor(ws: WebSocket) {
		this.ws = ws;
		ws.addEventListener("message", (event) => {
			const message = JSON.parse(String(event.data)) as {
				id?: number;
				error?: { message: string };
				result?: Record<string, unknown>;
				method?: string;
				params?: Record<string, unknown>;
			};

			if (message.id !== undefined) {
				const pending = this.pending.get(message.id);

				if (pending !== undefined) {
					this.pending.delete(message.id);
					if (message.error !== undefined) pending.reject(new Error(message.error.message));
					else pending.resolve(message.result ?? {});
				}

				return;
			}

			for (const handler of this.handlers.get(message.method ?? "") ?? []) {
				handler(message.params ?? {});
			}
		});
	}

	on(method: string, handler: (params: Record<string, unknown>) => void): () => void {
		const list = this.handlers.get(method) ?? [];
		list.push(handler);
		this.handlers.set(method, list);

		return () => {
			this.off(method, handler);
		};
	}

	off(method: string, handler: (params: Record<string, unknown>) => void): void {
		const list = this.handlers.get(method) ?? [];
		const index = list.indexOf(handler);

		if (index !== -1) list.splice(index, 1);
	}

	once(method: string): Promise<void> {
		let off: (() => void) | undefined;
		const event = new Promise<void>((resolveEvent) => {
			off = this.on(method, () => {
				off?.();
				resolveEvent();
			});
		});

		return withTimeout(event, 15_000, `CDP event ${method} timed out`).finally(() => off?.());
	}

	send(method: string, params: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
		const id = this.nextId;
		this.nextId += 1;
		const promise = new Promise<Record<string, unknown>>((resolveCall, rejectCall) => {
			this.pending.set(id, { resolve: resolveCall, reject: rejectCall });
		});

		this.ws.send(JSON.stringify({ id, method, params }));

		// A dead Chrome must not leave verification hanging on a CDP call.
		return withTimeout(promise, 15_000, `CDP call ${method} timed out`);
	}
}

async function renderBody(cdp: Cdp): Promise<string> {
	const result = await cdp.send("Runtime.evaluate", {
		expression: "document.body.innerHTML",
		returnByValue: true,
	});
	const value = (result.result ?? {}) as { value?: unknown };

	return String(value.value ?? "");
}

// ── Browser journey bookkeeping ─────────────────────────────────────────────

/**
 * Per-navigation request tracking: every phase resets the trace, so requests
 * observed in an earlier document can never satisfy a later phase's checks.
 * Console errors are run-wide, because hydration damage can surface after the
 * navigation that caused it.
 */
export class NetworkTrace {
	readonly entries: Array<{ url: string; status: number; mimeType: string }> = [];
	readonly consoleErrors: string[] = [];
	private readonly failed: string[] = [];
	private readonly mimeErrors: string[] = [];
	private readonly requestUrls = new Map<string, string>();
	private documentNavigations = 0;

	private readonly baseUrl: string;

	constructor(baseUrl: string) {
		this.baseUrl = baseUrl;
	}

	reset(): void {
		this.entries.length = 0;
		this.documentNavigations = 0;
	}

	observeRequest(requestId: string, url: string): void {
		this.requestUrls.set(requestId, url);
	}

	observeResponse(url: string, status: number, mimeType: string): void {
		this.entries.push({ url, status, mimeType });
	}

	observeFailure(requestId: string, errorText: string): void {
		const url = this.requestUrls.get(requestId) ?? "unknown url";

		if (url.startsWith(this.baseUrl)) {
			this.failed.push(`${errorText} ${url}`);
		}
	}

	observeConsoleError(text: string): void {
		this.consoleErrors.push(text);
	}

	observeMimeError(text: string): void {
		this.mimeErrors.push(text);
	}

	observeFrameNavigated(parentId: string | undefined): void {
		if (parentId === undefined) {
			this.documentNavigations += 1;
		}
	}

	hasEntry(suffix: string): boolean {
		return this.entries.some((entry) => entry.url.endsWith(suffix));
	}

	hasStatusEntry(suffix: string, status: number): boolean {
		return this.entries.some((entry) => entry.url.endsWith(suffix) && entry.status === status);
	}

	splitChunkRequests(): number {
		return this.entries.filter((entry) => entry.url.includes("split___")).length;
	}

	navigations(): number {
		return this.documentNavigations;
	}

	failures(): string[] {
		return [...this.failed];
	}

	mimeIssues(): string[] {
		return [...this.mimeErrors];
	}
}

async function waitForEntry(
	trace: NetworkTrace,
	suffix: string,
	timeoutMs: number,
): Promise<boolean> {
	const deadline = Date.now() + timeoutMs;

	while (Date.now() < deadline) {
		if (trace.hasEntry(suffix)) return true;

		await sleep(100);
	}

	return trace.hasEntry(suffix);
}

async function navigateToDocument(cdp: Cdp, trace: NetworkTrace, url: string): Promise<void> {
	trace.reset();
	const loaded = cdp.once("Page.loadEventFired");

	await cdp.send("Page.navigate", { url });
	await loaded;
}

async function clickSelector(cdp: Cdp, trace: NetworkTrace, selector: string): Promise<void> {
	trace.reset();
	await cdp.send("Runtime.evaluate", {
		expression: `document.querySelector('${selector}')?.click()`,
		awaitPromise: true,
	});
}

async function runBrowserJourney(options: {
	cdp: Cdp;
	baseUrl: string;
	token: string;
	identity: FixtureIdentity;
	log: CheckLog;
	out: (line: string) => void;
	settleMs: number;
}): Promise<void> {
	const { cdp, baseUrl, token, identity, log, out, settleMs } = options;
	const trace = new NetworkTrace(baseUrl);

	cdp.on("Runtime.consoleAPICalled", (params) => {
		if (params.type === "error") {
			const args = (params.args ?? []) as Array<{ value?: unknown; description?: string }>;

			trace.observeConsoleError(args.map((arg) => arg.value ?? arg.description ?? "").join(" "));
		}
	});
	cdp.on("Runtime.exceptionThrown", (params) => {
		const details = (params.exceptionDetails ?? {}) as {
			exception?: { description?: string };
			text?: string;
		};

		trace.observeConsoleError(
			details.exception?.description ?? details.text ?? "uncaught exception",
		);
	});
	cdp.on("Network.requestWillBeSent", (params) => {
		const request = (params.request ?? {}) as { url?: string };
		const requestId = String(params.requestId ?? "");

		if (request.url !== undefined) trace.observeRequest(requestId, request.url);
	});
	cdp.on("Network.loadingFailed", (params) => {
		trace.observeFailure(
			String(params.requestId ?? ""),
			String(params.errorText ?? "request failed"),
		);
	});
	cdp.on("Network.responseReceived", (params) => {
		const response = (params.response ?? {}) as {
			url?: string;
			status?: number;
			mimeType?: string;
		};
		const url = response.url ?? "";

		trace.observeResponse(url, response.status ?? 0, response.mimeType ?? "");

		if ((response.status ?? 0) >= 400 && url.startsWith(baseUrl)) {
			trace.observeFailure(String(params.requestId ?? ""), `${response.status}`);
		}
		if (
			url.includes("/pkg/") &&
			url.endsWith(".js") &&
			(response.mimeType ?? "").includes("javascript") === false &&
			response.status === 200
		) {
			trace.observeMimeError(`wrong MIME type ${response.mimeType} for ${url}`);
		}
	});
	cdp.on("Page.frameNavigated", (params) => {
		const frame = (params.frame ?? {}) as { parentId?: string };

		trace.observeFrameNavigated(frame.parentId);
	});

	await cdp.send("Runtime.enable");
	await cdp.send("Page.enable");
	await cdp.send("Network.enable");

	// Host-only session cookie, as the app sets it after sign-in; the browser
	// itself transports it on every navigation below.
	await cdp.send("Network.setCookie", {
		name: SESSION_COOKIE,
		value: token,
		url: baseUrl,
		path: "/",
		secure: true,
		httpOnly: true,
	});

	out("");
	out("browser navigation (session cookie transported by Chrome itself via CDP):");

	out("website entry:");
	await navigateToDocument(cdp, trace, `${baseUrl}/`);
	await waitForEntry(trace, "/pkg/site/monochange_app.js", 10_000);
	await sleep(settleMs);
	const landing = await renderBody(cdp);

	log.check(
		trace.hasStatusEntry("/pkg/site/monochange_app.js", 200),
		"the landing document loads the website entry module",
	);
	log.check(
		trace.hasStatusEntry("/pkg/site/monochange_app.css", 200),
		"the landing document loads the website stylesheet",
	);
	log.check(
		trace.hasEntry("/pkg/site/monochange_app_bg.wasm"),
		"the landing document fetches the website wasm module",
	);
	log.check(
		!trace.entries.some((entry) => entry.url.includes("/pkg/projects/")),
		"the landing document loads no projects bundle",
	);
	log.check(
		landing.includes(identity.login),
		`the session cookie carries the fixture identity into the website document (${identity.login})`,
	);
	log.check(
		landing.includes('href="/projects"') && landing.includes('target="_self"'),
		"the signed-in website nav links to /projects as a document navigation",
	);

	out("cross-frontend navigation (website → projects):");
	await clickSelector(cdp, trace, 'a[href="/projects"]');
	await waitForEntry(trace, "/pkg/projects/monochange_projects_web.js", 10_000);
	await sleep(settleMs);
	const projects = await renderBody(cdp);

	log.check(trace.navigations() >= 1, "clicking /projects performed a document navigation");
	log.check(
		trace.hasStatusEntry("/pkg/projects/monochange_projects_web.js", 200),
		"the projects document loads the projects entry module",
	);
	log.check(
		trace.hasStatusEntry("/pkg/site/monochange_app.css", 200),
		"the projects document loads the shared stylesheet",
	);
	log.check(
		!trace.entries.some((entry) => entry.url.endsWith("/pkg/site/monochange_app.js")),
		"the projects document loads no website entry module",
	);
	log.check(
		projects.includes('aria-label="Main navigation"'),
		"the projects entry renders the shared navigation",
	);
	log.check(
		projects.includes(identity.orgLogin),
		`the session cookie carries the seeded organization identity into the projects document (${identity.orgLogin})`,
	);

	out("cross-frontend navigation (projects → website):");
	await clickSelector(cdp, trace, 'a[href="/dashboard"]');
	await waitForEntry(trace, "/pkg/site/monochange_app.js", 10_000);
	await sleep(settleMs);
	const dashboard = await renderBody(cdp);

	log.check(trace.navigations() >= 1, "clicking /dashboard performed a document navigation");
	log.check(
		trace.hasStatusEntry("/pkg/site/monochange_app.js", 200),
		"the dashboard document loads the website entry module again",
	);
	log.check(
		dashboard.includes(identity.login) || dashboard.includes(identity.repoName),
		"the dashboard renders the connected account or its fixture repository",
	);

	out("lazy navigation inside one entry:");
	await clickSelector(cdp, trace, 'a[href="/install"]');
	await sleep(settleMs);
	const install = await renderBody(cdp);

	log.check(
		trace.navigations() === 0,
		"the /install link stays client-side inside the website entry",
	);
	log.check(/[Ii]nstall/.test(install), "the lazy route rendered without a document reload");
	log.check(trace.splitChunkRequests() > 0, "the lazy route loaded its own split chunk");

	out("asset and console hygiene (whole run):");
	log.check(
		trace.consoleErrors.length === 0,
		`no console or hydration errors (${trace.consoleErrors.join(" | ") || "none"})`,
	);
	log.check(
		trace.failures().length === 0,
		`no failed first-party requests (${trace.failures().join(" | ") || "none"})`,
	);
	log.check(
		trace.mimeIssues().length === 0,
		`no wrong-MIME bundle responses (${trace.mimeIssues().join(" | ") || "none"})`,
	);
}

// ── Orchestration with explicit cleanup ownership ────────────────────────────

async function removeWorkDir(workDir: string): Promise<string[]> {
	try {
		// Retry because Chrome releases its profile directory asynchronously
		// on some platforms; every owned process is confirmed stopped before
		// this runs.
		rmSync(workDir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });

		return [];
	} catch (error) {
		return [
			`could not remove the temporary work dir ${workDir}: ${error instanceof Error ? error.message : String(error)}`,
		];
	}
}

/**
 * Runs the whole verification and returns the process exit code. Verification
 * failures, startup failures, and cleanup failures all return 1; success is
 * only reported after cleanup succeeded. Every spawned process is registered
 * before anything is awaited, so a partial startup still gets cleaned up.
 */
export async function executeVerification(config: VerificationConfig): Promise<number> {
	const out = config.out ?? ((line: string) => console.log(line));
	const err = config.err ?? ((line: string) => console.error(line));
	const registry = new OwnedProcesses();
	const log = new CheckLog(out, err);
	let browser: { cdp: Cdp; ws: WebSocket } | null = null;

	try {
		const port = await freePort();
		const server = await startOwnedServer({
			registry,
			command: config.server.command,
			args: config.server.args,
			cwd: config.server.cwd,
			env: config.server.env,
			port,
			timeoutMs: config.startupTimeoutMs ?? DEFAULT_STARTUP_TIMEOUT_MS,
		});
		const token = mintSessionToken(config.jwtSecret, config.identity);

		config.seedDatabase?.(config.databaseFile);
		await runRawSsrPhase({
			baseUrl: server.baseUrl,
			token,
			identity: config.identity,
			log,
			out,
		});

		if (config.chrome !== undefined) {
			const chromePort = await freePort();
			const wsUrl = await startOwnedChrome({
				registry,
				command: config.chrome.command,
				workDir: config.workDir,
				port: chromePort,
				timeoutMs: config.chromeTimeoutMs ?? DEFAULT_CHROME_TIMEOUT_MS,
			});
			const ws = new WebSocket(wsUrl);
			const cdp = new Cdp(ws);
			browser = { cdp, ws };

			await withTimeout(
				new Promise<void>((resolveOpen, rejectOpen) => {
					ws.addEventListener("open", () => resolveOpen(), { once: true });
					ws.addEventListener("error", () => rejectOpen(new Error("could not connect to Chrome")), {
						once: true,
					});
				}),
				config.chromeTimeoutMs ?? DEFAULT_CHROME_TIMEOUT_MS,
				"the CDP WebSocket did not open",
			);

			await runBrowserJourney({
				cdp,
				baseUrl: server.baseUrl,
				token,
				identity: config.identity,
				log,
				out,
				settleMs: config.settleMs ?? DEFAULT_SETTLE_MS,
			});
		} else {
			out("");
			out("browser navigation: skipped (no Chrome configured; server-side phases only)");
		}
	} catch (error) {
		const message = error instanceof Error ? (error.stack ?? error.message) : String(error);

		err(message);
		log.fail(
			`the verification run could not complete: ${error instanceof Error ? error.message : String(error)}`,
		);
	}

	// Cleanup always runs, also after a partial startup, and always before
	// any success claim.
	const cleanupErrors: string[] = [];

	if (browser !== null) {
		try {
			// Ask Chrome to shut itself down gracefully before any signal.
			await withTimeout(browser.cdp.send("Browser.close"), 5000, "Browser.close timed out");
		} catch {
			// A Chrome that already exited cannot answer; the registry still
			// owns terminating it below.
		}

		try {
			await withTimeout(
				new Promise<void>((resolveClose) => {
					const ws = browser?.ws;

					if (ws === undefined || ws.readyState === WebSocket.CLOSED) {
						resolveClose();
						return;
					}

					ws.addEventListener("close", () => resolveClose(), { once: true });
					ws.close();
				}),
				3000,
				"the CDP WebSocket did not close",
			);
		} catch (error) {
			cleanupErrors.push(
				`the CDP WebSocket did not close cleanly: ${error instanceof Error ? error.message : String(error)}`,
			);
		}
	}

	cleanupErrors.push(...(await registry.shutdown()));

	// The temporary Chrome profile lives in the work dir; it is only removed
	// after every owned process that touched it is confirmed stopped.
	if (cleanupErrors.length === 0) {
		cleanupErrors.push(...(await removeWorkDir(config.workDir)));
	}

	for (const cleanupError of cleanupErrors) {
		err(`  cleanup FAIL: ${cleanupError}`);
	}

	if (log.failures.length > 0) {
		err(`${log.failures.length} verification failure(s); see the FAIL lines above`);

		return 1;
	}

	if (cleanupErrors.length > 0) {
		err(`${cleanupErrors.length} cleanup failure(s); nothing is claimed verified`);

		return 1;
	}

	out(
		config.chrome !== undefined
			? "\nweb navigation verification passed (raw SSR identity and browser navigation, after clean shutdown)"
			: "\nserver-side verification passed (raw SSR identity; browser phases skipped, after clean shutdown)",
	);

	return 0;
}

// ── Production wiring ───────────────────────────────────────────────────────

function randomIdentity(): FixtureIdentity {
	const suffix = randomBytes(4).toString("hex");
	const number = (offset: number): number =>
		offset + Number.parseInt(randomBytes(3).toString("hex"), 16);

	return {
		userId: 1,
		githubId: number(770_000_000),
		login: `verify-nav-${suffix}`,
		orgGithubId: number(880_000_000),
		orgLogin: `verify-nav-org-${suffix}`,
		installationGithubId: number(990_000_000),
		repoGithubId: number(660_000_000),
		repoName: `repo-${suffix}`,
		projectSlug: `harness-${suffix}`,
		projectName: `verify project ${suffix}`,
	};
}

/** SQL string literal for harness-generated fixture values. */
function sqlString(value: string): string {
	return `'${value.replaceAll("'", "''")}'`;
}

/**
 * Resolves the sqlite3 CLI: the devenv shell PATH does not always include
 * /usr/bin, so the usual absolute locations are checked before falling back
 * to a bare PATH lookup.
 */
function findSqlite3(): string {
	const candidates = [
		process.env.SQLITE3_BIN,
		"/usr/bin/sqlite3",
		"/opt/homebrew/bin/sqlite3",
		"/usr/local/bin/sqlite3",
	];
	const found = candidates.find((candidate) => candidate !== undefined && existsSync(candidate));

	return found ?? "sqlite3";
}

/**
 * Seeds the fixture identity through the real migration-created schema. The
 * installation is a `User`-account installation linked to an organization
 * row, which is the one shape that renders organization and project identity
 * without any GitHub API call: `Organization` account types would need a
 * live ownership check, and the GitHub App is deliberately absent here.
 */
function seedAppDatabase(databaseFile: string, identity: FixtureIdentity): void {
	const statement = [
		`INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (${identity.userId}, ${identity.githubId}, ${sqlString(identity.login)}, 'test-only-verify-token');`,
		`INSERT INTO organizations (github_id, github_login) VALUES (${identity.orgGithubId}, ${sqlString(identity.orgLogin)});`,
		`INSERT INTO installations (user_id, github_installation_id, github_account_login, github_account_type, target_type, organization_id)`,
		`  SELECT ${identity.userId}, ${identity.installationGithubId}, ${sqlString(identity.orgLogin)}, 'User', 'selected', id FROM organizations WHERE github_login = ${sqlString(identity.orgLogin)};`,
		`INSERT INTO repositories (installation_id, github_repo_id, github_full_name)`,
		`  SELECT i.id, ${identity.repoGithubId}, ${sqlString(`${identity.orgLogin}/${identity.repoName}`)} FROM installations i WHERE i.github_installation_id = ${identity.installationGithubId};`,
		`INSERT INTO projects (organization_id, slug, name, description, created_by_user_id)`,
		`  SELECT o.id, ${sqlString(identity.projectSlug)}, ${sqlString(identity.projectName)}, 'verify-web-navigation fixture', ${identity.userId} FROM organizations o WHERE o.github_login = ${sqlString(identity.orgLogin)};`,
	].join("\n");
	const result = spawnSync(findSqlite3(), [databaseFile, statement], { encoding: "utf8" });

	if (result.error !== undefined || result.status !== 0) {
		const output = result.stderr || result.stdout || `exit status ${String(result.status)}`;
		const detail = result.error?.message ?? output;

		throw new Error(`could not seed the fixture user into ${databaseFile}:\n${detail}`);
	}
}

function parseOptions(args: string[]): { site_root?: string; server_bin?: string } {
	const names = new Set(["--site-root", "--server-bin"]);
	const options: { site_root?: string; server_bin?: string } = {};

	for (let index = 0; index < args.length; index += 1) {
		const key = args[index];

		if (!names.has(key)) {
			throw new Error(`unknown argument: ${key} (supported: --site-root, --server-bin)`);
		}

		const value = args[index + 1];

		if (value === undefined) {
			throw new Error(`missing value for ${key}`);
		}

		options[key.slice(2).replaceAll("-", "_") as "site_root" | "server_bin"] = value;
		index += 1;
	}

	return options;
}

async function main(): Promise<number> {
	const options = parseOptions(process.argv.slice(2));
	const serverBin = resolve(repoRoot, options.server_bin ?? "app/target/release/monochange_app");
	const siteRoot = resolve(repoRoot, options.site_root ?? "app/target/site");

	if (!existsSync(serverBin)) {
		throw new Error(
			`missing server binary ${serverBin}; build both frontends first (build:web --release)`,
		);
	}

	if (!existsSync(siteRoot)) {
		throw new Error(
			`missing site root ${siteRoot}; build both frontends first (build:web --release)`,
		);
	}

	const chrome = findChrome();
	const workDir = mkdtempSync(join(tmpdir(), "verify-web-navigation-"));
	const jwtSecret = `verify-web-navigation-${randomBytes(8).toString("hex")}`;
	const identity = randomIdentity();
	const databaseFile = join(workDir, "verify.sqlite3");

	return executeVerification({
		workDir,
		databaseFile,
		server: {
			command: serverBin,
			args: [],
			cwd: join(repoRoot, "app"),
			env: {
				...process.env,
				MONOSECRET_PROFILE: "ci",
				DATABASE_URL: `sqlite://${databaseFile}`,
				JWT_SECRET: jwtSecret,
				TOKEN_ENCRYPTION_KEY: "verify-web-navigation-token-encryption-key",
				GITHUB_CLIENT_ID: "verify-navigation-client-id",
				GITHUB_CLIENT_SECRET: "verify-navigation-client-secret",
				LEPTOS_ENV: "PROD",
				LEPTOS_SITE_ROOT: siteRoot,
				LEPTOS_SITE_PKG_DIR: "pkg",
				RUST_LOG: "error",
			},
		},
		seedDatabase: (file) => {
			seedAppDatabase(file, identity);
		},
		chrome: { command: chrome },
		jwtSecret,
		identity,
	});
}

// Run the CLI only when this file is the entry module, so tests can import
// the helpers. The entry path is canonicalized because the ESM loader
// resolves import.meta.url through real paths (macOS /var → /private/var),
// while argv[1] keeps the invocation path.
const entryPath = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined;

if (entryPath !== undefined && import.meta.url === entryPath) {
	main()
		.then((code) => {
			// Everything this run owns is already stopped and the work dir
			// removed; the explicit exit keeps the code deterministic.
			process.exit(code);
		})
		.catch((error: unknown) => {
			console.error(error instanceof Error ? (error.stack ?? error.message) : String(error));
			process.exit(1);
		});
}
