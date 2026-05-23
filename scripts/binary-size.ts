#!/usr/bin/env node
/**
 * Binary size comparison for PRs.
 *
 * Builds the release binary from both main and the PR branch,
 * measures their sizes, and produces a markdown comment summarizing
 * the delta. Intended to run as part of CI alongside the existing
 * benchmark-binary job.
 *
 * Usage:
 *   node scripts/binary-size.ts compare \
 *     --main-bin /tmp/mc-main \
 *     --pr-bin /tmp/mc-pr \
 *     --output /tmp/size-comment.md
 *
 *   node scripts/binary-size.ts compare \
 *     --main-bin /tmp/mc-main \
 *     --main-size-bytes 30_886_048 \
 *     --pr-bin /tmp/mc-pr \
 *     --output /tmp/size-comment.md
 *
 * When --main-size-bytes is provided, the main binary itself is not
 * probed for its size (useful when the main binary was already measured
 * in a prior step).
 */

import { statSync, writeFileSync } from "node:fs";

const KIB = 1024;
const MIB = KIB * 1024;

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
		if (value === undefined || value.startsWith("--")) die(`expected value for ${key}`);
		options[key.replace(/^--/, "").replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = value;
		index += 1;
	}
	return options;
}

function fileSize(path) {
	try {
		return statSync(path).size;
	} catch {
		die(`cannot stat ${path}`);
	}
}

function formatBytes(bytes) {
	if (bytes >= MIB) return `${(bytes / MIB).toFixed(2)} MiB`;
	if (bytes >= KIB) return `${(bytes / KIB).toFixed(2)} KiB`;
	return `${bytes} B`;
}

function formatDelta(current, previous) {
	const delta = current - previous;
	const sign = delta > 0 ? "+" : "";
	return `${sign}${formatBytes(Math.abs(delta))} (${sign}${((delta / previous) * 100).toFixed(1)}%)`;
}

function emojiForDelta(current, previous) {
	const delta = current - previous;
	if (delta < -KIB) return "📉";
	if (delta > KIB) return "📈";
	return "➡️";
}

function compareMode(args) {
	const o = parseOptions(args, ["--main-bin", "--main-size-bytes", "--pr-bin", "--output"]);
	if (!o.prBin || !o.output) {
		die("compare requires --pr-bin and --output");
	}
	if (!o.mainBin && !o.mainSizeBytes) {
		die("compare requires either --main-bin or --main-size-bytes");
	}

	const prSize = fileSize(o.prBin);
	const mainSize = o.mainSizeBytes ? Number(o.mainSizeBytes) : fileSize(o.mainBin);

	const lines = [
		"## Binary Size: main vs PR",
		"",
		"| Binary | Size | Delta |",
		"|---|---|---|",
		`| main | ${formatBytes(mainSize)} | — |`,
	];

	const delta = prSize - mainSize;
	const emoji = emojiForDelta(prSize, mainSize);

	if (delta > KIB) {
		lines.push(`| PR | ${formatBytes(prSize)} | ${emoji} ${formatDelta(prSize, mainSize)} |`);
		lines.push("");
		lines.push(
			`> ⚠️ **This PR increases the binary size by ${formatBytes(delta)} (${((delta / mainSize) * 100).toFixed(1)}%).** Consider whether new dependencies or features can be feature-gated or replaced with lighter alternatives.`,
		);
	} else if (delta < -KIB) {
		lines.push(`| PR | ${formatBytes(prSize)} | ${emoji} ${formatDelta(prSize, mainSize)} |`);
		lines.push("");
		lines.push(
			`> ✅ **This PR reduces the binary size by ${formatBytes(Math.abs(delta))} (${((delta / mainSize) * 100).toFixed(1)}%).** Nice work!`,
		);
	} else {
		lines.push(`| PR | ${formatBytes(prSize)} | ${emoji} ${formatDelta(prSize, mainSize)} |`);
		lines.push("");
		lines.push("> No meaningful change in binary size.");
	}

	writeFileSync(o.output, `${lines.join("\n")}\n`);
	console.log(`Binary size comparison written to ${o.output}`);
	console.log(`  main: ${formatBytes(mainSize)}`);
	console.log(`  PR:   ${formatBytes(prSize)}`);
	console.log(`  delta: ${formatDelta(prSize, mainSize)}`);
}

const [mode, ...args] = process.argv.slice(2);
try {
	if (mode === "compare") compareMode(args);
	else die(`usage: ${process.argv[1]} <compare> [args...]`);
} catch (error) {
	console.error(error instanceof Error ? error.message : String(error));
	process.exit(1);
}
