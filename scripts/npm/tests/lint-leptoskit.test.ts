import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
	chmodSync,
	cpSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, test } from "vitest";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const scriptPath = join(repoRoot, "scripts/lint-leptoskit.sh");

const pinned = {
	git: "https://github.com/leptoskit/leptoskit",
	rev: "a93214edc8e5f6c7999c628285093f15cf188639",
	version: "0.3.0",
	bin: "leptoskit",
};

type MetadataEntry = {
	version: string;
	git: string;
	rev: string;
	locked: boolean;
	bins: string[];
};

type LockPackage = {
	name: string;
	version?: string;
	source?: string | null;
};

type FakeTools = {
	binDir: string;
	cargoLog: string;
};

type CachedBinaryOptions = {
	source?: "git" | "registry";
	rev?: string;
	version?: string;
	bin?: string;
	toolchain?: string;
	manifest?: "both" | "crates-toml-only";
};

type RunOptions = {
	env?: Record<string, string>;
	cliLog?: string;
};

function writeExecutable(path: string, content: string) {
	writeFileSync(path, content, { mode: 0o755 });
	chmodSync(path, 0o755);
}

function readLines(path: string): string[] {
	return readFileSync(path, "utf8")
		.split("\n")
		.filter((line) => line.length > 0);
}

function gitSource(options: { git?: string; rev?: string; version?: string } = {}) {
	return `git+${options.git ?? pinned.git}?branch=main#${options.rev ?? pinned.rev}`;
}

function lockfile(packages: LockPackage[], options: { git?: string; rev?: string } = {}) {
	const lines = ["version = 3", ""];

	for (const pkg of packages) {
		lines.push(
			"[[package]]",
			`name = "${pkg.name}"`,
			`version = "${pkg.version ?? pinned.version}"`,
		);

		if (pkg.source !== null) {
			lines.push(`source = "${pkg.source ?? gitSource(options)}"`);
		}

		lines.push("");
	}

	return lines.join("\n");
}

function writeLockfiles(
	root: string,
	packages: LockPackage[],
	options: { git?: string; rev?: string } = {},
) {
	for (const path of ["app/Cargo.lock", "app/native/Cargo.lock", "extension/Cargo.lock"]) {
		writeFileSync(join(root, path), lockfile(packages, options));
	}
}

function createFixtureRoot() {
	const root = mkdtempSync(join(tmpdir(), "lint-leptoskit-"));

	mkdirSync(join(root, "scripts"), { recursive: true });
	cpSync(scriptPath, join(root, "scripts", "lint-leptoskit.sh"));

	for (const dir of ["app", "app/native", "extension"]) {
		mkdirSync(join(root, dir), { recursive: true });
		writeFileSync(join(root, dir, "leptoskit.toml"), "[lint]\n");
	}

	return root;
}

function createFakeTools(
	root: string,
	options: {
		metadata?: MetadataEntry;
		rustRelease?: string;
		rustHost?: string;
	} = {},
): FakeTools {
	const binDir = join(root, "fake-bin");
	const metadataPath = join(root, "metadata.json");
	const cargoLog = join(root, "cargo.log");

	mkdirSync(binDir, { recursive: true });
	writeFileSync(
		metadataPath,
		JSON.stringify({
			metadata: {
				bin: {
					leptoskit_cli:
						options.metadata ??
						({
							version: pinned.version,
							git: pinned.git,
							rev: pinned.rev,
							locked: true,
							bins: [pinned.bin],
						} satisfies MetadataEntry),
				},
			},
		}),
	);
	writeFileSync(cargoLog, "");
	writeExecutable(
		join(binDir, "rustc"),
		`#!/usr/bin/env bash
printf 'release: ${options.rustRelease ?? "1.97.0-nightly"}\\nhost: ${options.rustHost ?? "aarch64-apple-darwin"}\\n'
`,
	);
	writeExecutable(
		join(binDir, "cargo"),
		`#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >> "${cargoLog}"

if [[ "$1" == "metadata" ]]; then
	cat "${metadataPath}"
	exit 0
fi

if [[ "$1" != "install" ]]; then
	printf 'unexpected cargo invocation: %s\\n' "$*" >&2
	exit 2
fi

if [[ "\${FAKE_INSTALL_FAIL:-}" == "1" ]]; then
	exit 1
fi

root=""
version=""
rev=""
git_url=""
bin_name=""
package=""
while [[ $# -gt 0 ]]; do
	case "$1" in
		--root)
			root="$2"
			shift 2
			;;
		--version)
			version="$2"
			shift 2
			;;
		--rev)
			rev="$2"
			shift 2
			;;
		--git)
			git_url="$2"
			shift 2
			;;
		--bin)
			bin_name="$2"
			shift 2
			;;
		--locked | --force)
			shift
			;;
		*)
			package="$1"
			shift
			;;
	esac
done

if [[ -n "\${FAKE_INSTALL_REV:-}" ]]; then
	rev="\${FAKE_INSTALL_REV}"
fi

mkdir -p "$root/bin"
cat > "$root/bin/$bin_name" <<'CLI'
#!/usr/bin/env bash
printf '%s\\t%s\\n' "$PWD" "$*" >> "\${CLI_LOG}"
CLI
chmod +x "$root/bin/$bin_name"

source="git+\${git_url}?rev=\${rev}#\${rev}"
cat > "$root/.crates2.json" <<EOF
{"installs":{"\${package} \${version} (\${source})":{"bins":["\${bin_name}"]}}}
EOF
cat > "$root/.crates.toml" <<EOF
[v1]
"\${package} \${version} (\${source})" = [
	"\${bin_name}",
]
EOF

if [[ -n "\${CLI_LOG:-}" ]]; then
	printf 'install\\n' >> "\${CLI_LOG}"
fi
`,
	);

	return { binDir, cargoLog };
}

function createCachedBinary(root: string, options: CachedBinaryOptions = {}) {
	const version = options.version ?? pinned.version;
	const bin = options.bin ?? pinned.bin;
	const toolchain = options.toolchain ?? "rust-nightly";
	const rev = options.rev ?? pinned.rev;
	const installDir = join(root, ".bin", toolchain, "leptoskit_cli", version);
	const source =
		options.source === "registry"
			? "registry+https://github.com/rust-lang/crates.io-index"
			: `git+${pinned.git}?rev=${rev}#${rev}`;
	const key = `leptoskit_cli ${version} (${source})`;

	mkdirSync(join(installDir, "bin"), { recursive: true });
	writeExecutable(
		join(installDir, "bin", bin),
		`#!/usr/bin/env bash
printf '%s\\t%s\\n' "$PWD" "$*" >> "\${CLI_LOG}"
`,
	);
	writeFileSync(
		join(installDir, ".crates2.json"),
		JSON.stringify({ installs: { [key]: { bins: [bin] } } }),
	);
	writeFileSync(join(installDir, ".crates.toml"), `[v1]\n"${key}" = [\n    "${bin}",\n]\n`);

	if (options.manifest === "crates-toml-only") {
		rmSync(join(installDir, ".crates2.json"));
	}

	return installDir;
}

function runLint(root: string, tools: FakeTools, options: RunOptions = {}) {
	const cliLog = options.cliLog ?? join(root, "cli.log");

	writeFileSync(cliLog, "");

	const result = spawnSync("bash", [join(root, "scripts", "lint-leptoskit.sh")], {
		cwd: root,
		encoding: "utf8",
		env: {
			...process.env,
			PATH: `${tools.binDir}:${process.env.PATH ?? ""}`,
			CLI_LOG: cliLog,
			...options.env,
		},
	});

	return {
		result,
		installCalls: readLines(tools.cargoLog).filter((line) => line.startsWith("install ")),
		cliEntries: readLines(cliLog),
	};
}

const defaultPackages: LockPackage[] = [
	{ name: "monochange_app", source: null },
	{ name: "leptoskit" },
	{ name: "leptoskit_rpc" },
];

describe("lint-leptoskit resolver", () => {
	test("reuses a cached binary only when its install manifest proves the pinned Git revision", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root);

		writeLockfiles(root, defaultPackages);
		createCachedBinary(root);

		const { result, installCalls, cliEntries } = runLint(root, tools);

		assert.equal(result.status, 0, result.stderr);
		assert.equal(installCalls.length, 0);
		assert.deepEqual(
			cliEntries.sort(),
			[
				`${join(root, "app")}\tlint --deny-warnings --no-clippy`,
				`${join(root, "app/native")}\tlint --deny-warnings --no-clippy`,
				`${join(root, "extension")}\tlint --deny-warnings --no-clippy`,
			].sort(),
		);
	});

	test("accepts lockfile sources that pin the revision through a rev query", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root);

		// Production lockfiles carry `?rev=<pin>#<pin>` sources because every
		// manifest pins the immutable revision instead of a moving branch.
		writeLockfiles(
			root,
			defaultPackages.map((pkg) => ({
				...pkg,
				source: `git+${pinned.git}?rev=${pinned.rev}#${pinned.rev}`,
			})),
		);
		createCachedBinary(root);

		const { result, installCalls, cliEntries } = runLint(root, tools);

		assert.equal(result.status, 0, result.stderr);
		assert.equal(installCalls.length, 0);
		assert.equal(cliEntries.length, 3);
	});

	test("reinstalls a cached registry binary with the same version", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root);

		writeLockfiles(root, defaultPackages);
		const installDir = createCachedBinary(root, { source: "registry" });

		const { result, installCalls, cliEntries } = runLint(root, tools);

		assert.equal(result.status, 0, result.stderr);
		assert.equal(installCalls.length, 1);
		assert.deepEqual(installCalls[0]?.split(" "), [
			"install",
			"--locked",
			"--force",
			"--root",
			installDir,
			"--git",
			pinned.git,
			"--rev",
			pinned.rev,
			"--version",
			pinned.version,
			"--bin",
			pinned.bin,
			"leptoskit_cli",
		]);
		assert.equal(cliEntries[0], "install");
		assert.equal(cliEntries.length, 4);
	});

	test("requires the Git install entry to own the binary being run", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root);

		writeLockfiles(root, defaultPackages);
		const installDir = createCachedBinary(root);
		const gitKey = `leptoskit_cli ${pinned.version} (git+${pinned.git}?rev=${pinned.rev}#${pinned.rev})`;
		const registryKey = `leptoskit_cli ${pinned.version} (registry+https://github.com/rust-lang/crates.io-index)`;

		writeFileSync(
			join(installDir, ".crates2.json"),
			JSON.stringify({
				installs: {
					[registryKey]: { bins: [pinned.bin] },
					[gitKey]: { bins: ["lk"] },
				},
			}),
		);

		const { result, installCalls } = runLint(root, tools);

		assert.equal(result.status, 0, result.stderr);
		assert.equal(installCalls.length, 1);
	});

	test("reinstalls a cached binary installed from a different Git revision", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root);

		writeLockfiles(root, defaultPackages);
		createCachedBinary(root, { rev: "b".repeat(40) });

		const { result, installCalls } = runLint(root, tools);

		assert.equal(result.status, 0, result.stderr);
		assert.equal(installCalls.length, 1);
		assert.match(installCalls[0] ?? "", new RegExp(`--rev ${pinned.rev} `));
	});

	test("rejects a lockfile that keeps leptoskit on the registry", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root);

		writeLockfiles(root, defaultPackages);
		writeFileSync(
			join(root, "app/Cargo.lock"),
			lockfile([
				{ name: "leptoskit", source: "registry+https://github.com/rust-lang/crates.io-index" },
				{ name: "leptoskit_rpc" },
			]),
		);
		createCachedBinary(root);

		const { result, installCalls, cliEntries } = runLint(root, tools);

		assert.equal(result.status, 1);
		assert.match(result.stderr, /app\/Cargo\.lock has leptoskit 0\.3\.0 from 'registry\+/);
		assert.equal(installCalls.length, 0);
		assert.deepEqual(cliEntries, []);
	});

	test("rejects one drifted leptoskit package even when another matches", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root);

		writeLockfiles(root, defaultPackages);
		writeFileSync(
			join(root, "app/Cargo.lock"),
			lockfile([
				{ name: "leptoskit" },
				{ name: "leptoskit_rpc", source: gitSource({ rev: "c".repeat(40) }) },
			]),
		);
		createCachedBinary(root);

		const { result, installCalls, cliEntries } = runLint(root, tools);

		assert.equal(result.status, 1);
		assert.match(result.stderr, /app\/Cargo\.lock has leptoskit_rpc 0\.3\.0 from 'git\+/);
		assert.equal(installCalls.length, 0);
		assert.deepEqual(cliEntries, []);
	});

	test("never runs the cached CLI when the forced install fails", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root);

		writeLockfiles(root, defaultPackages);
		createCachedBinary(root, { source: "registry" });

		const { result, installCalls, cliEntries } = runLint(root, tools, {
			env: { FAKE_INSTALL_FAIL: "1" },
		});

		assert.equal(result.status, 1);
		assert.match(result.stderr, /cargo install failed for leptoskit_cli/);
		assert.equal(installCalls.length, 1);
		assert.deepEqual(cliEntries, []);
	});

	test("reads git, rev, version, and bin from cargo metadata instead of duplicating them", () => {
		const root = createFixtureRoot();
		const metadata = {
			version: "4.5.6",
			git: "https://example.test/leptoskit",
			rev: "1".repeat(40),
			locked: true,
			bins: ["leptoskit-custom"],
		};
		const tools = createFakeTools(root, { metadata });

		writeLockfiles(
			root,
			defaultPackages.map((pkg) => ({ ...pkg, version: metadata.version })),
			{ git: metadata.git, rev: metadata.rev },
		);

		const { result, installCalls, cliEntries } = runLint(root, tools);

		assert.equal(result.status, 0, result.stderr);
		assert.equal(installCalls.length, 1);
		assert.deepEqual(installCalls[0]?.split(" "), [
			"install",
			"--locked",
			"--force",
			"--root",
			join(root, ".bin", "rust-nightly", "leptoskit_cli", metadata.version),
			"--git",
			metadata.git,
			"--rev",
			metadata.rev,
			"--version",
			metadata.version,
			"--bin",
			"leptoskit-custom",
			"leptoskit_cli",
		]);
		assert.equal(cliEntries.length, 4);
	});

	test("falls back to .crates.toml when .crates2.json is absent", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root);

		writeLockfiles(root, defaultPackages);
		createCachedBinary(root, { manifest: "crates-toml-only" });

		const { result, installCalls } = runLint(root, tools);

		assert.equal(result.status, 0, result.stderr);
		assert.equal(installCalls.length, 0);
	});

	test("uses the host triple for stable toolchains like cargo-run-bin", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root, {
			rustRelease: "1.99.0",
			rustHost: "aarch64-apple-darwin",
		});

		writeLockfiles(root, defaultPackages);
		createCachedBinary(root, { toolchain: "rust-aarch64-apple-darwin" });

		const { result, installCalls } = runLint(root, tools);

		assert.equal(result.status, 0, result.stderr);
		assert.equal(installCalls.length, 0);
	});

	test("refuses to run an install whose manifest names the wrong Git revision", () => {
		const root = createFixtureRoot();
		const tools = createFakeTools(root);

		writeLockfiles(root, defaultPackages);
		createCachedBinary(root, { source: "registry" });

		const { result, cliEntries } = runLint(root, tools, {
			env: { FAKE_INSTALL_REV: "d".repeat(40) },
		});

		assert.equal(result.status, 1);
		assert.match(result.stderr, /does not prove git/);
		assert.equal(
			cliEntries.some((entry) => entry.includes("\tlint ")),
			false,
		);
	});
});
