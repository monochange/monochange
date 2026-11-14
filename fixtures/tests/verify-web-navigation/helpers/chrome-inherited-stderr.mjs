#!/usr/bin/env node
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import http from "node:http";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

if (process.argv.includes("--hold-stderr")) {
	setTimeout(() => process.exit(0), 30_000);
} else {
	const profile = process.argv.find((arg) => arg.startsWith("--user-data-dir="))?.split("=")[1];
	const port = Number(process.argv.find((arg) => arg.startsWith("--remote-debugging-port="))?.split("=")[1]);
	if (!profile || !port) throw new Error("Missing fixture profile or port");
	mkdirSync(profile, { recursive: true });
	const helper = spawn(process.execPath, [fileURLToPath(import.meta.url), "--hold-stderr"], {
		stdio: ["ignore", "ignore", "inherit"],
	});
	writeFileSync(join(profile, "helper.pid"), String(helper.pid));
	writeFileSync(join(profile, "chrome.pid"), String(process.pid));
	process.stderr.write("fixture browser diagnostic\n");
	process.on("SIGTERM", () => process.exit(0));
	http.createServer((_request, response) => {
		response.setHeader("content-type", "application/json");
		response.end(JSON.stringify([{ type: "page", webSocketDebuggerUrl: "ws://127.0.0.1/fixture" }]));
	}).listen(port, "127.0.0.1");
}
