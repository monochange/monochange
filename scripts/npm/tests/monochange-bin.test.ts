import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "vitest";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const launcherPath = join(process.cwd(), "packages/monochange__cli/bin/monochange.js");

function createSandbox() {
	return mkdtempSync(join(tmpdir(), "monochange-bin-"));
}

function createRoot(root) {
	mkdirSync(join(root, "bin"), { recursive: true });
	writeFileSync(
		join(root, "package.json"),
		JSON.stringify({
			name: "monochange-bin-test",
			private: true,
			type: "module",
		}),
	);
	cpSync(launcherPath, join(root, "bin", "monochange.js"));
}

function hostPackageName() {
	return process.platform === "darwin"
		? `@monochange/cli-darwin-${process.arch}`
		: process.platform === "linux"
			? `@monochange/cli-linux-${process.arch === "arm64" ? "arm64" : "x64"}-gnu`
			: `@monochange/cli-win32-${process.arch === "arm64" ? "arm64" : "x64"}-msvc`;
}

function createPackage(root, pkgName, binaryContent, mode = 0o755) {
	const packageDir = join(root, "node_modules", ...pkgName.split("/"));
	const binDir = join(packageDir, "bin");
	mkdirSync(binDir, { recursive: true });
	writeFileSync(
		join(packageDir, "package.json"),
		JSON.stringify({ name: pkgName, version: "1.0.0" }),
	);

	if (process.platform === "win32") {
		writeFileSync(join(binDir, "monochange.exe"), binaryContent);
	} else {
		writeFileSync(join(binDir, "monochange"), binaryContent, { mode });
	}

	return join(binDir, process.platform === "win32" ? "monochange.exe" : "monochange");
}

function runLauncher(root, args = []) {
	return spawnSync("node", [join(root, "bin", "monochange.js"), ...args], {
		cwd: root,
		encoding: "utf8",
	});
}

test("launcher executes the installed platform binary", () => {
	const root = createSandbox();
	createRoot(root);
	const pkgName = hostPackageName();
	const binary =
		process.platform === "win32"
			? "@echo off\r\necho launcher-ok %*\r\n"
			: '#!/bin/sh\necho launcher-ok "$@"\n';
	createPackage(root, pkgName, binary);

	const result = runLauncher(root, ["--help"]);
	assert.equal(result.status, 0);
	assert.match(result.stdout, /launcher-ok/);
});

test("launcher reports a clear error when no compatible binary is installed", () => {
	const root = createSandbox();
	createRoot(root);

	const result = runLauncher(root, ["--help"]);
	assert.notEqual(result.status, 0);
	assert.match(result.stderr, /Unable to find a compatible monochange binary/);
});

test.skipIf(process.platform === "win32")(
	"launcher restores a missing executable bit before running the binary",
	() => {
		const root = createSandbox();
		createRoot(root);
		const binaryPath = createPackage(
			root,
			hostPackageName(),
			'#!/bin/sh\necho launcher-ok "$@"\n',
			0o644,
		);

		const result = runLauncher(root, ["--version"]);
		assert.equal(result.status, 0, result.stderr);
		assert.match(result.stdout, /launcher-ok --version/);
		assert.equal(statSync(binaryPath).mode & 0o111, 0o111);
	},
);
