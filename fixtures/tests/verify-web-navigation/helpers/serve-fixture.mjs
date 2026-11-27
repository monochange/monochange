#!/usr/bin/env node
// Throwaway fixture HTTP server for the verify-web-navigation lifecycle
// tests. Serves committed HTML fixtures by cookie state so the harness's raw
// SSR phase can be exercised end to end over real HTTP without the Rust
// backend: /health answers 200, and each configured path serves the fixture
// file matching the request's cookie state — no cookie (anonymous), a JWT
// whose payload carries the expected login (signed in), or anything else
// (invalid). Writes its pid to VERIFY_PIDFILE so tests can observe that the
// harness reaped the process it owns. Exits 2 if the port is taken, like a
// real backend that cannot bind.
import { readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";

const port = Number(process.env.PORT ?? "0");
const pidfile = process.env.VERIFY_PIDFILE;
const htmlDir = process.env.VERIFY_HTML_DIR ?? ".";
const expectedLogin = process.env.VERIFY_LOGIN ?? "";
const cookieName = process.env.VERIFY_COOKIE_NAME ?? "__Host-monochange_session";
const routes = JSON.parse(process.env.VERIFY_ROUTES ?? "{}");

if (pidfile !== undefined && pidfile !== "") {
	writeFileSync(pidfile, `${process.pid}\n`);
}

function cookieState(header) {
	const cookie = (header ?? "")
		.split(";")
		.map((part) => part.trim())
		.find((part) => part.startsWith(`${cookieName}=`));

	if (cookie === undefined) return "anonymous";

	const value = cookie.slice(cookieName.length + 1);
	const payload = value.split(".")[1];

	try {
		const claims = JSON.parse(Buffer.from(payload ?? "", "base64url").toString("utf8"));

		return claims.github_login === expectedLogin ? "signedIn" : "invalid";
	} catch {
		return "invalid";
	}
}

const server = createServer((request, response) => {
	if (request.url === "/health") {
		response.writeHead(200, { "content-type": "application/json" });
		response.end('{"status":"ok"}');

		return;
	}

	const state = cookieState(request.headers.cookie);
	const file = routes[request.url ?? ""]?.[state];

	if (file === undefined) {
		response.writeHead(500, { "content-type": "text/plain" });
		response.end(`no fixture for ${request.url} in state ${state}`);

		return;
	}

	try {
		response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
		response.end(readFileSync(join(htmlDir, file)));
	} catch (error) {
		response.writeHead(500, { "content-type": "text/plain" });
		response.end(String(error));
	}
});

server.once("error", (error) => {
	console.error(`serve-fixture could not listen on ${port}: ${error}`);
	process.exit(2);
});

server.listen(port, "127.0.0.1", () => {
	console.log(`serve-fixture listening on 127.0.0.1:${port}`);
});
