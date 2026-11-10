#!/usr/bin/env node
// Headless-browser verification that the two browser frontends behave as one
// site: each page loads the entry bundle of the frontend that owns its path,
// links that cross between website and projects perform real document
// navigations, lazy navigation inside one entry stays client-side, the
// session cookie carries identity across those navigations, and no console,
// hydration, MIME, or failed-request errors occur.
//
// This is the project's isolated browser tooling: it launches a throwaway
// headless Chrome with its own user-data-dir and talks CDP over WebSocket
// (Node built-ins only, no preview tools, no personal browser profile). The
// production server binary serves the assembled leptoskit site with dummy
// test credentials and a seeded SQLite database — no secret access, no
// network calls outside localhost.
//
// Usage:
//   pnpm node scripts/verify-web-navigation.ts \
//     [--site-root app/target/site] [--server-bin app/target/release/monochange_app]
//
// CHROME_PATH selects the Chrome/Chromium binary; the default looks in the
// usual install locations.
//
// The session token is minted here with the same HS256 claims schema as
// `monochange_app_api::create_token` (sub, github_id, github_login, exp, iat,
// epoch) using the dummy secret this script hands the server; if that schema
// drifts, the signed-in assertions fail loudly rather than silently passing.
import { spawn, spawnSync } from "node:child_process";
import { createHmac } from "node:crypto";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import net from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..");
const JWT_SECRET = "verify-web-navigation-signing-key";
const SESSION_COOKIE = "__Host-monochange_session";

function die(message) {
	console.error(message);
	process.exit(1);
}

function parseOptions(args, names) {
	const options = {};

	for (let index = 0; index < args.length; index += 1) {
		const key = args[index];

		if (!names.includes(key)) die(`unknown argument: ${key}`);
		const value = args[index + 1];

		if (value === undefined) die(`missing value for ${key}`);
		options[key.slice(2).replaceAll("-", "_")] = value;
		index += 1;
	}

	return options;
}

// ── Session token (HS256, claims mirror monochange_app_api::create_token) ──
function base64url(input) {
	return Buffer.from(input).toString("base64url");
}

function mintSessionToken(user) {
	const now = Math.floor(Date.now() / 1000);
	const header = base64url(JSON.stringify({ alg: "HS256", typ: "JWT" }));
	const claims = base64url(
		JSON.stringify({
			sub: user.id,
			github_id: user.githubId,
			github_login: user.login,
			exp: now + 7 * 24 * 60 * 60,
			iat: now,
			epoch: 0,
		}),
	);
	const signingInput = `${header}.${claims}`;
	const signature = createHmac("sha256", JWT_SECRET).update(signingInput).digest("base64url");

	return `${signingInput}.${signature}`;
}

// ── Server fixture ──────────────────────────────────────────────────────────
function waitForHealth(baseUrl, timeoutMs) {
	const deadline = Date.now() + timeoutMs;
	return new Promise<void>((resolveHealth, reject) => {
		const attempt = async () => {
			try {
				const response = await fetch(`${baseUrl}/health`);
				if (response.ok) {
					resolveHealth();
					return;
				}
			} catch {
				// The server is not listening yet.
			}
			if (Date.now() > deadline) {
				reject(new Error(`server did not become healthy within ${timeoutMs} ms`));
				return;
			}

			setTimeout(attempt, 200);
		};
		attempt();
	});
}

async function startServer(options, workDir) {
	const port = await freePort();
	const databaseUrl = `sqlite://${join(workDir, "verify.sqlite3")}`;
	const serverBin = resolve(repoRoot, options.server_bin ?? "app/target/release/monochange_app");

	if (!existsSync(serverBin)) {
		die(`missing server binary ${serverBin}; build both frontends first (build:web --release)`);
	}

	const child = spawn(serverBin, [], {
		cwd: join(repoRoot, "app"),
		env: {
			...process.env,
			PORT: String(port),
			MONOSECRET_PROFILE: "ci",
			DATABASE_URL: databaseUrl,
			JWT_SECRET: JWT_SECRET,
			TOKEN_ENCRYPTION_KEY: "verify-web-navigation-token-encryption-key",
			GITHUB_CLIENT_ID: "verify-navigation-client-id",
			GITHUB_CLIENT_SECRET: "verify-navigation-client-secret",
			LEPTOS_ENV: "PROD",
			LEPTOS_SITE_ROOT: resolve(repoRoot, options.site_root ?? "app/target/site"),
			LEPTOS_SITE_PKG_DIR: "pkg",
			RUST_LOG: "error",
		},
		stdio: ["ignore", "pipe", "pipe"],
	});
	child.stderr.on("data", (chunk) => {
		// Keep the server's stderr out of the verification report but fail
		// loudly on a startup panic.
		const text = String(chunk);
		if (text.includes("panicked")) {
			die(`server panicked:\n${text.slice(0, 2000)}`);
		}
	});
	const baseUrl = `http://127.0.0.1:${port}`;

	await waitForHealth(baseUrl, 30_000);

	return { child, baseUrl, databaseUrl, port };
}

function seedUser(databaseUrl, user) {
	// The server creates the schema when it runs its migrations; insert the
	// fixture user afterwards with the macOS/Linux sqlite3 CLI.
	const file = databaseUrl.replace("sqlite://", "");
	const statement = `INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (${user.id}, ${user.githubId}, '${user.login}', 'test-only-verify-token');`;
	const result = spawnSync("sqlite3", [file, statement], { encoding: "utf8" });

	if (result.status !== 0) {
		die(`could not seed the fixture user:\n${result.stderr || result.stdout}`);
	}
}

// ── Chrome + CDP ────────────────────────────────────────────────────────────
function findChrome() {
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

	if (!found) {
		die("no Chrome/Chromium found; set CHROME_PATH to the browser binary");
	}

	return found;
}

async function launchChrome(workDir) {
	const chrome = findChrome();
	const port = await freePort();
	const child = spawn(
		chrome,
		[
			"--headless=new",
			"--no-sandbox",
			"--disable-gpu",
			"--no-first-run",
			"--disable-extensions",
			`--remote-debugging-port=${port}`,
			`--user-data-dir=${join(workDir, "chrome-profile")}`,
			"about:blank",
		],
		{ stdio: ["ignore", "ignore", "pipe"] },
	);
	// Give the DevTools endpoint a moment, then discover the page target.
	const deadline = Date.now() + 20_000;

	while (Date.now() < deadline) {
		try {
			const response = await fetch(`http://127.0.0.1:${port}/json/list`);
			const targets = (await response.json()) as Array<{
				type?: string;
				webSocketDebuggerUrl?: string;
			}>;

			const page = targets.find((target) => target.type === "page");

			if (page?.webSocketDebuggerUrl) {
				return { child, wsUrl: page.webSocketDebuggerUrl };
			}
		} catch {
			// Chrome is still starting.
		}
		await new Promise((wait) => setTimeout(wait, 200));
	}

	die("Chrome's DevTools endpoint never came up");
}

class Cdp {
	private readonly ws: WebSocket;
	private nextId = 1;
	private readonly handlers = new Map<string, Array<(params: Record<string, unknown>) => void>>();
	private readonly pending = new Map<
		number,
		{ resolve: (value: Record<string, unknown>) => void; reject: (error: Error) => void }
	>();

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
				if (pending) {
					this.pending.delete(message.id);
					if (message.error) pending.reject(new Error(message.error.message));
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

		return () => this.off(method, handler);
	}

	off(method: string, handler: (params: Record<string, unknown>) => void): void {
		const list = this.handlers.get(method) ?? [];
		const index = list.indexOf(handler);

		if (index !== -1) list.splice(index, 1);
	}

	once(method: string): Promise<void> {
		return new Promise<void>((resolveEvent) => {
			const off = this.on(method, () => {
				off();
				resolveEvent();
			});
		});
	}

	send(method: string, params: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
		const id = this.nextId;
		this.nextId += 1;
		const promise = new Promise<Record<string, unknown>>((resolveCall, rejectCall) => {
			this.pending.set(id, { resolve: resolveCall, reject: rejectCall });
		});
		this.ws.send(JSON.stringify({ id, method, params }));

		return promise;
	}
}

// ── Verification ────────────────────────────────────────────────────────────
const failures = [];
function check(condition, message) {
	if (condition) {
		console.log(`  ok: ${message}`);
	} else {
		failures.push(message);
		console.error(`  FAIL: ${message}`);
	}
}

function freePort() {
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

async function main() {
	const options = parseOptions(process.argv.slice(2), ["--site-root", "--server-bin"]);
	const workDir = mkdtempSync(join(tmpdir(), "verify-web-navigation-"));
	const user = { id: 1, githubId: 101, login: "alice" };
	const token = mintSessionToken(user);

	const { child: server, baseUrl, databaseUrl } = await startServer(options, workDir);
	seedUser(databaseUrl, user);
	const { child: chrome, wsUrl } = await launchChrome(workDir);

	let ws = null;
	try {
		ws = new WebSocket(wsUrl);
		await new Promise<void>((resolveOpen, rejectOpen) => {
			ws.addEventListener("open", resolveOpen);
			ws.addEventListener("error", () => rejectOpen(new Error("could not connect to Chrome")));
		});
		const cdp = new Cdp(ws);

		const consoleErrors = [];
		const failedRequests = [];
		const mimeErrors = [];
		const requests = [];
		cdp.on("Runtime.consoleAPICalled", (params) => {
			if (params.type === "error") {
				const args = (params.args ?? []) as Array<{ value?: unknown; description?: string }>;
				consoleErrors.push(args.map((arg) => arg.value ?? arg.description ?? "").join(" "));
			}
		});
		cdp.on("Runtime.exceptionThrown", (params) => {
			const details = (params.exceptionDetails ?? {}) as {
				exception?: { description?: string };
				text?: string;
			};
			consoleErrors.push(details.exception?.description ?? details.text ?? "uncaught exception");
		});
		cdp.on("Network.loadingFailed", (params) => {
			failedRequests.push(params.errorText ?? "request failed");
		});
		cdp.on("Network.responseReceived", (params) => {
			const response = (params.response ?? {}) as {
				url?: string;
				status?: number;
				mimeType?: string;
			};
			const url = response.url ?? "";
			const status = response.status ?? 0;
			const mimeType = response.mimeType ?? "";
			requests.push({ url, status, mimeType });
			if (status >= 400) {
				failedRequests.push(`${status} ${url}`);
			}
			if (
				url.includes("/pkg/") &&
				url.endsWith(".js") &&
				!mimeType.includes("javascript") &&
				status === 200
			) {
				mimeErrors.push(`${mimeType} for ${url}`);
			}
		});

		await cdp.send("Runtime.enable");
		await cdp.send("Page.enable");
		await cdp.send("Network.enable");

		// Host-only session cookie, as the app sets it after sign-in.
		await cdp.send("Network.setCookie", {
			name: SESSION_COOKIE,
			value: token,
			url: baseUrl,
			path: "/",
			secure: true,
			httpOnly: true,
		});

		let lastContextId = null;
		let documentNavigations = 0;
		cdp.on("Runtime.executionContextCreated", (params) => {
			const context = (params.context ?? {}) as { id?: number };
			if (lastContextId !== null && context.id !== lastContextId) {
				documentNavigations += 1;
			}

			lastContextId = context.id;
		});

		const navigate = async (url) => {
			const loaded = cdp.once("Page.loadEventFired");
			await cdp.send("Page.navigate", { url });
			await loaded;
		};

		console.log("website entry:");
		requests.length = 0;
		await navigate(`${baseUrl}/`);
		await sleep(1200);
		const landing = await renderBody(cdp);
		check(
			requests.some(
				(request) => request.url.endsWith("/pkg/site/monochange_app.js") && request.status === 200,
			),
			"the landing page loads the website entry module",
		);
		check(
			requests.some(
				(request) => request.url.endsWith("/pkg/site/monochange_app.css") && request.status === 200,
			),
			"the landing page loads the website stylesheet",
		);
		check(
			requests.some((request) => request.url.endsWith("/pkg/site/monochange_app_bg.wasm")),
			"the landing page fetches the website wasm module",
		);

		check(
			!requests.some((request) => request.url.includes("/pkg/projects/")),
			"the landing page loads no projects bundle",
		);
		check(
			landing.includes("Signed in as alice"),
			"the session cookie carries identity into the website SSR render",
		);
		check(
			landing.includes('href="/projects"') && landing.includes('target="_self"'),
			"the signed-in website nav links to /projects with a document navigation",
		);

		console.log("cross-frontend navigation (website → projects):");
		requests.length = 0;
		documentNavigations = 0;
		// A real click on the nav link, exactly as a user does.
		await cdp.send("Runtime.evaluate", {
			expression: "document.querySelector('a[href=\"/projects\"]').click()",
			awaitPromise: true,
		});
		await sleep(1500);
		const projects = await renderBody(cdp);
		check(documentNavigations >= 1, "clicking /projects performed a document navigation");
		check(
			requests.some(
				(request) =>
					request.url.endsWith("/pkg/projects/monochange_projects.js") && request.status === 200,
			),
			"the product page loads the projects entry module",
		);

		check(
			requests.some(
				(request) =>
					request.url.endsWith("/pkg/projects/monochange_projects.css") && request.status === 200,
			),
			"the product page loads the projects stylesheet",
		);
		check(
			!requests.some((request) => request.url.endsWith("/pkg/site/monochange_app.js")),
			"the product page loads no website entry module",
		);
		check(
			projects.includes("Projects navigation"),
			"the projects entry renders the product navigation",
		);
		check(
			projects.includes("Signed in as alice") || projects.includes("Sign out"),
			"the session cookie carries identity into the projects SSR render",
		);

		console.log("cross-frontend navigation (projects → website):");
		requests.length = 0;
		documentNavigations = 0;
		await cdp.send("Runtime.evaluate", {
			expression: "document.querySelector('a[href=\"/dashboard\"]').click()",
			awaitPromise: true,
		});
		await sleep(1500);
		const dashboard = await renderBody(cdp);
		check(documentNavigations >= 1, "clicking /dashboard performed a document navigation");
		check(
			requests.some((request) => request.url.endsWith("/pkg/site/monochange_app.js")),
			"the repositories dashboard loads the website entry module again",
		);

		check(
			dashboard.includes("Connect repositories on GitHub") || dashboard.includes("alice"),
			"the dashboard renders the connected account",
		);

		console.log("lazy navigation inside one entry:");
		requests.length = 0;
		documentNavigations = 0;
		const lazySplitRequests = () => requests.filter((request) => request.url.includes("split___"));
		await cdp.send("Runtime.evaluate", {
			expression: "document.querySelector('a[href=\"/install\"]').click()",
			awaitPromise: true,
		});
		await sleep(1500);
		const install = await renderBody(cdp);
		check(
			documentNavigations === 0,
			"the /install link stays client-side inside the website entry",
		);

		check(
			install.includes("Install") || install.includes("install"),
			"the lazy route rendered without a document reload",
		);
		check(lazySplitRequests().length > 0, "the lazy route loaded its own split chunk");

		console.log("asset and console hygiene:");
		check(consoleErrors.length === 0, `no console errors (${consoleErrors.join(" | ") || "none"})`);
		check(
			failedRequests.length === 0,
			`no failed requests (${failedRequests.join(" | ") || "none"})`,
		);
		check(
			mimeErrors.length === 0,
			`no wrong-MIME bundle responses (${mimeErrors.join(" | ") || "none"})`,
		);

		if (failures.length > 0) {
			die(`${failures.length} verification failure(s); see the FAIL lines above`);
		}

		console.log("\nweb navigation verification passed");
	} finally {
		chrome.kill();
		server.kill();
		ws?.close();
		// Give the children a moment to release the work dir before cleanup.
		await new Promise((wait) => setTimeout(wait, 300));

		rmSync(workDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
	}
}

function sleep(ms) {
	return new Promise((resolveWait) => setTimeout(resolveWait, ms));
}

async function renderBody(cdp) {
	const result = await cdp.send("Runtime.evaluate", {
		expression: "document.body.innerHTML",
		returnByValue: true,
	});

	return String(result.result?.value ?? "");
}

main().catch((error) => {
	die(error instanceof Error ? (error.stack ?? error.message) : String(error));
});
