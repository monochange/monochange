#!/usr/bin/env node
// Runs Lighthouse against every core monochange.dev flow and fails when page
// metrics or built asset sizes regress past scripts/website-budgets.json.
//
// Usage:
//   pnpm node scripts/benchmark-website.ts run \
//     [--site-root app/target/site] [--server-bin app/target/release/monochange_app] \
//     [--budgets scripts/website-budgets.json] [--routes /,/install] \
//     [--output report.md] [--results results.json] [--base-url http://127.0.0.1:3000]
//
// The server is started with throwaway credentials against a temporary SQLite
// database, mirroring the environment app-ci uses for cargo test. Set
// MONOCHANGE_ALLOW_BENCHMARK_BUDGET_FAILURES=true to record violations
// without failing (matches the CLI benchmark job's escape hatch).

import { spawn, spawnSync } from "node:child_process";
import { createWriteStream } from "node:fs";
import { readdirSync, readFileSync, statSync } from "node:fs";
import net from "node:net";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..");

const METRICS = [
	{ key: "ttfbMs", audit: "server-response-time", label: "TTFB", format: "ms" },
	{ key: "fcpMs", audit: "first-contentful-paint", label: "FCP", format: "ms" },
	{ key: "lcpMs", audit: "largest-contentful-paint", label: "LCP", format: "ms" },
	{ key: "tbtMs", audit: "total-blocking-time", label: "TBT", format: "ms" },
	{ key: "cls", audit: "cumulative-layout-shift", label: "CLS", format: "score" },
];

function die(message) {
	console.error(message);
	process.exit(1);
}

function parseOptions(args, names) {
	const options: Record<string, string> = {};
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

function readJson(path) {
	try {
		return JSON.parse(readFileSync(path, "utf8"));
	} catch (error) {
		die(`could not read ${path}: ${error.message}`);
	}
}

function formatValue(value, format) {
	if (value === undefined) return "—";
	return format === "score" ? value.toFixed(3) : `${Math.round(value)} ms`;
}

function listSiteFiles(siteRoot) {
	const pkgDir = join(siteRoot, "pkg");
	let entries;
	try {
		entries = readdirSync(pkgDir);
	} catch {
		die(`missing ${pkgDir}; build the site first with cargo leptos build --release`);
	}
	return entries
		.filter((name) => statSync(join(pkgDir, name)).isFile())
		.map((name) => {
			const path = join(pkgDir, name);
			const raw = readFileSync(path);
			return { name, bytes: raw.byteLength, gzipBytes: gzipSync(raw).byteLength };
		})
		.toSorted((left, right) => right.bytes - left.bytes);
}

function checkAssetBudgets(budgets, files, violations) {
	const totals = files.reduce(
		(acc, file) => {
			acc.totalBytes += file.bytes;
			if (file.name.endsWith(".wasm")) acc.wasmBytes += file.bytes;
			if (file.name.endsWith(".js")) acc.jsBytes += file.bytes;
			if (file.name.endsWith(".css")) acc.cssBytes += file.bytes;
			return acc;
		},
		{ totalBytes: 0, wasmBytes: 0, jsBytes: 0, cssBytes: 0 },
	);
	const mainWasm = files.find((file) => file.name === "monochange_app.wasm");
	const limits = [
		["pkgTotalBytes", totals.totalBytes],
		["pkgWasmBytes", totals.wasmBytes],
		["pkgJsBytes", totals.jsBytes],
		["pkgCssBytes", totals.cssBytes],
		["pkgEagerWasmBytes", mainWasm?.bytes ?? 0],
	];
	for (const [key, actual] of limits) {
		const budget = budgets.assets?.[key];
		if (budget !== undefined && actual > budget) {
			violations.push(`${key} ${actual} exceeds budget ${budget}`);
		}
	}
	return totals;
}

function freePort() {
	return new Promise((resolvePort, reject) => {
		const server = net.createServer();
		server.unref();
		server.on("error", reject);
		server.listen(0, "127.0.0.1", () => {
			const address = server.address();
			if (typeof address === "object" && address !== null) {
				server.close(() => resolvePort(address.port));
			} else {
				reject(new Error("could not bind a local port"));
			}
		});
	});
}

async function waitForHealth(baseUrl, timeoutMs) {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		try {
			const response = await fetch(`${baseUrl}/health`);
			if (response.ok) return;
		} catch {
			// Server not ready yet.
		}
		await new Promise((sleep) => setTimeout(sleep, 500));
	}
	die(`server did not become healthy within ${timeoutMs} ms at ${baseUrl}`);
}

function startServer(options, port, databaseUrl) {
	// Resolve against the repository root so a relative --server-bin survives
	// the app/ working directory the server needs for monosecret resolution.
	const serverBin = resolve(repoRoot, options.server_bin ?? "app/target/release/monochange_app");
	const child = spawn(serverBin, [], {
		cwd: join(repoRoot, "app"),
		env: {
			...process.env,
			PORT: String(port),
			MONOSECRET_PROFILE: "ci",
			DATABASE_URL: databaseUrl,
			JWT_SECRET: "website-benchmark-signing-key",
			GITHUB_CLIENT_ID: "benchmark-client-id",
			GITHUB_CLIENT_SECRET: "benchmark-client-secret",
			LEPTOS_ENV: "PROD",
			LEPTOS_SITE_ROOT: resolve(repoRoot, options.site_root ?? "app/target/site"),
			LEPTOS_SITE_PKG_DIR: "pkg",
			RUST_LOG: "warn",
		},
		stdio: ["ignore", "pipe", "pipe"],
	});
	let log = "";
	child.stdout.on("data", (chunk) => {
		log += chunk;
	});
	child.stderr.on("data", (chunk) => {
		log += chunk;
	});
	child.on("exit", (code) => {
		if (code !== null && code !== 0) {
			console.error(`server exited with ${code}:\n${log.slice(-4000)}`);
		}
	});
	return { child, log: () => log };
}

function lighthouseBin() {
	if (process.env.MONOCHANGE_LIGHTHOUSE_BIN) return process.env.MONOCHANGE_LIGHTHOUSE_BIN;
	const local = join(repoRoot, "node_modules/.bin/lighthouse");
	return existsSync(local) ? local : "lighthouse";
}

function existsSync(path) {
	try {
		statSync(path);
		return true;
	} catch {
		return false;
	}
}

function runLighthouse(baseUrl, route, preset) {
	const url = `${baseUrl}${route}`;
	const chromeFlags = process.env.MONOCHANGE_CHROME_FLAGS ?? "--headless=new --no-sandbox";
	const result = spawnSync(
		lighthouseBin(),
		[
			url,
			"--output=json",
			"--output-path=stdout",
			"--quiet",
			`--preset=${preset}`,
			"--only-categories=performance,best-practices",
			"--max-wait-for-load=60000",
			`--chrome-flags=${chromeFlags}`,
		],
		{ encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
	);
	if (result.status !== 0) {
		const detail = result.stderr || result.stdout || `exit code ${result.status}`;
		die(`lighthouse failed for ${url}:\n${detail.slice(0, 4000)}`);
	}
	const start = result.stdout.indexOf("{");
	const end = result.stdout.lastIndexOf("}");
	if (start === -1 || end === -1) {
		die(`lighthouse printed no JSON for ${url}:\n${result.stdout.slice(0, 2000)}`);
	}
	return JSON.parse(result.stdout.slice(start, end + 1));
}

function collectRouteMetrics(lhr) {
	const metrics: Record<string, number | undefined> = {};
	for (const metric of METRICS) {
		const audit = lhr.audits?.[metric.audit];
		if (audit?.numericValue !== undefined) metrics[metric.key] = audit.numericValue;
	}
	metrics.performance = lhr.categories?.performance?.score ?? undefined;
	return metrics;
}

function consoleErrors(lhr) {
	const items = lhr.audits?.["errors-in-console"]?.details?.items ?? [];
	return items
		.map((item) => item.messages?.join(" ") ?? item.description ?? "console error")
		.filter((message) => message.length > 0);
}

function checkRouteBudgets(route, budget, metrics, violations) {
	for (const metric of [
		...METRICS,
		{ key: "performance", label: "Performance", format: "score" },
	]) {
		const limit = budget[metric.key];
		const actual = metrics[metric.key];
		if (limit === undefined || actual === undefined) continue;
		// Only the performance score is higher-is-better; timings and CLS are
		// maximum budgets.
		const breached = metric.key === "performance" ? actual < limit : actual > limit;
		if (breached) {
			const rendered = formatValue(actual, metric.format);
			const limitRendered = formatValue(limit, metric.format);
			violations.push(`${route}: ${metric.label} ${rendered} breached budget ${limitRendered}`);
		}
	}
}

async function warmRoute(baseUrl, route) {
	try {
		await fetch(`${baseUrl}${route}`);
	} catch {
		// Lighthouse will surface a hard failure if the route is truly broken.
	}
}

function writeReport(output, lines) {
	if (!output) {
		console.log(lines.join("\n"));
		return;
	}
	createWriteStream(output).write(`${lines.join("\n")}\n`);
}

async function run(options) {
	const budgetsPath = options.budgets ?? join(scriptDir, "website-budgets.json");
	const budgets = readJson(budgetsPath);
	const preset = budgets.preset ?? "desktop";
	const violations = [];

	const files = listSiteFiles(resolve(repoRoot, options.site_root ?? "app/target/site"));
	const totals = checkAssetBudgets(budgets, files, violations);

	const routeEntries = Object.entries(budgets.routes ?? {});
	const routesFilter = options.routes ? options.routes.split(",").map((r) => r.trim()) : null;
	const routes = routesFilter
		? routeEntries.filter(([route]) => routesFilter.includes(route))
		: routeEntries;
	if (routes.length === 0) die("no routes selected; check --routes against the budgets file");

	let baseUrl = options.base_url;
	let server;
	if (!baseUrl) {
		const port = await freePort();
		baseUrl = `http://127.0.0.1:${port}`;
		const databaseUrl = `sqlite:${join(
			repoRoot,
			`app/target/website-benchmark-${process.pid}.sqlite3`,
		)}`;
		server = startServer(options, port, databaseUrl);
		await waitForHealth(baseUrl, 60_000);
	}

	const results: {
		preset: string;
		routes: Record<string, Record<string, number | undefined>>;
		consoleErrors: Record<string, string[]>;
		assets: {
			files: { name: string; bytes: number; gzipBytes: number }[];
			totals: { totalBytes: number; wasmBytes: number; jsBytes: number; cssBytes: number };
		};
	} = { preset, routes: {}, consoleErrors: {}, assets: { files, totals } };
	for (const [route] of routes) {
		await warmRoute(baseUrl, route);
		const lhr = runLighthouse(baseUrl, route, preset);
		const metrics = collectRouteMetrics(lhr);
		results.routes[route] = metrics;
		checkRouteBudgets(route, budgets.routes[route], metrics, violations);
		const errors = consoleErrors(lhr);
		if (errors.length > 0) {
			results.consoleErrors[route] = errors;
			violations.push(
				`${route}: browser console errors (likely broken hydration): ${errors.join(" | ")}`,
			);
		}
	}

	if (server) {
		server.child.kill("SIGTERM");
	}

	const lines = ["## Website benchmark results", "", "### Lighthouse (desktop preset)", ""];
	lines.push(`| Route | Performance | ${METRICS.map((m) => m.label).join(" | ")} |`);
	lines.push(`| --- | --- | ${METRICS.map(() => "---").join(" | ")} |`);
	for (const [route] of routes) {
		const metrics = results.routes[route];
		const performanceScore = metrics.performance;
		const score = performanceScore === undefined ? "—" : `${Math.round(performanceScore * 100)}`;
		lines.push(
			`| ${route} | ${score} | ${METRICS.map((m) => formatValue(metrics[m.key], m.format)).join(" | ")} |`,
		);
	}
	lines.push("", "### Built assets", "");
	lines.push("| File | Bytes | Gzip |");
	lines.push("| --- | --- | --- |");
	for (const file of files.slice(0, 15)) {
		lines.push(`| \`${file.name}\` | ${file.bytes} | ${file.gzipBytes} |`);
	}
	lines.push(
		`Total: ${totals.totalBytes} bytes (wasm ${totals.wasmBytes}, js ${totals.jsBytes}, css ${totals.cssBytes})`,
	);

	if (options.results) {
		createWriteStream(options.results).write(`${JSON.stringify(results, null, "\t")}\n`);
	}

	if (violations.length > 0) {
		lines.push("", "### Budget violations", "");
		lines.push(...violations.map((violation) => `- ${violation}`));
	}

	writeReport(options.output, lines);
	console.log(lines.join("\n"));

	if (violations.length > 0) {
		if (process.env.MONOCHANGE_ALLOW_BENCHMARK_BUDGET_FAILURES === "true") {
			console.error(
				"\nBudget violations recorded but ignored (MONOCHANGE_ALLOW_BENCHMARK_BUDGET_FAILURES=true).",
			);
			return;
		}
		die(
			`\n${violations.length} website budget violation(s); update scripts/website-budgets.json only alongside a deliberate trade-off.`,
		);
	}
}

async function main() {
	const [command, ...rest] = process.argv.slice(2);
	if (command !== "run")
		die("usage: benchmark-website.ts run [--site-root ...] [--server-bin ...]");
	const options = parseOptions(rest, [
		"--site-root",
		"--server-bin",
		"--budgets",
		"--routes",
		"--output",
		"--results",
		"--base-url",
	]);
	await run(options);
}

main().catch((error) => {
	die(error instanceof Error ? (error.stack ?? error.message) : String(error));
});
