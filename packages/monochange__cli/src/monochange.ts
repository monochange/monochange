#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const PLATFORM_PACKAGES = {
	darwin: {
		arm64: ["@monochange/cli-darwin-arm64"],
		x64: ["@monochange/cli-darwin-x64"],
	},
	linux: {
		arm64: ["@monochange/cli-linux-arm64-gnu", "@monochange/cli-linux-arm64-musl"],
		x64: ["@monochange/cli-linux-x64-gnu", "@monochange/cli-linux-x64-musl"],
	},
	win32: {
		arm64: ["@monochange/cli-win32-arm64-msvc"],
		x64: ["@monochange/cli-win32-x64-msvc"],
	},
};

function getCandidatePackages() {
	return PLATFORM_PACKAGES[process.platform]?.[process.arch] ?? [];
}

function resolveBinary(pkgName) {
	try {
		const packageJsonPath = require.resolve(`${pkgName}/package.json`);
		const packageDir = path.dirname(packageJsonPath);
		const binaryName = process.platform === "win32" ? "monochange.exe" : "monochange";
		const binaryPath = path.join(packageDir, "bin", binaryName);

		if (fs.existsSync(binaryPath)) {
			return binaryPath;
		}
	} catch {
		// Ignore missing optional dependencies and continue trying candidates.
	}

	return null;
}

function isExecutable(filePath) {
	try {
		fs.accessSync(filePath, fs.constants.X_OK);
		return true;
	} catch {
		return false;
	}
}

// A platform package whose binary lost its executable bit (as every
// @monochange/cli 0.20.0 platform package did) would otherwise fail with
// EACCES. Restore the bit in place, or run a private executable copy when the
// install directory belongs to another user.
function prepareBinary(binaryPath) {
	if (process.platform === "win32" || isExecutable(binaryPath)) {
		return { binaryPath, cleanup() {} };
	}

	try {
		fs.chmodSync(binaryPath, 0o755);
		return { binaryPath, cleanup() {} };
	} catch {
		// The install directory is read-only for this user.
	}

	const copyDir = fs.mkdtempSync(path.join(os.tmpdir(), "monochange-"));
	const copyPath = path.join(copyDir, path.basename(binaryPath));
	fs.copyFileSync(binaryPath, copyPath);
	fs.chmodSync(copyPath, 0o755);

	return {
		binaryPath: copyPath,
		cleanup() {
			fs.rmSync(copyDir, { recursive: true, force: true });
		},
	};
}

function shouldTryNextPackage(result) {
	if (result.error) {
		return true;
	}

	if (result.status !== 127) {
		return false;
	}

	const stderr = result.stderr ?? "";

	return /not found|no such file or directory|exec format error/i.test(stderr);
}

function forwardOutput(result) {
	if (result.stdout) {
		process.stdout.write(result.stdout);
	}

	if (result.stderr) {
		process.stderr.write(result.stderr);
	}
}

function main() {
	const candidates = getCandidatePackages();

	if (candidates.length === 0) {
		console.error(
			`monochange does not currently publish npm binaries for ${process.platform}/${process.arch}. ` +
				"Install from GitHub releases or with `cargo install monochange` instead.",
		);
		process.exit(1);
	}

	const failures = [];

	for (const pkgName of candidates) {
		const binaryPath = resolveBinary(pkgName);

		if (!binaryPath) {
			continue;
		}

		const prepared = prepareBinary(binaryPath);
		const result = spawnSync(prepared.binaryPath, process.argv.slice(2), {
			encoding: "utf8",
			stdio: ["inherit", "pipe", "pipe"],
			windowsHide: false,
		});
		prepared.cleanup();

		if (shouldTryNextPackage(result)) {
			const detail = result.error?.message ?? result.stderr?.trim() ?? "failed to launch";
			failures.push(`${pkgName}: ${detail}`);
			continue;
		}

		forwardOutput(result);
		process.exit(result.status ?? 0);
	}

	console.error("Unable to find a compatible monochange binary in the installed npm packages.");
	console.error(`Tried: ${candidates.join(", ")}`);

	if (failures.length > 0) {
		console.error(failures.join("\n"));
	}

	console.error(
		"Reinstall with `npm install -g @monochange/cli`, download a binary from GitHub releases, or use `cargo install monochange`.",
	);
	process.exit(1);
}

main();
