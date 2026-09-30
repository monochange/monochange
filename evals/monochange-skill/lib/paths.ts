// Filesystem layout and process helpers for the monochange skill evaluation
// harness.
//
// Everything the harness creates lives under `evals/monochange-skill/.work/`,
// which is git-ignored: run workdirs are throwaway monorepos that each carry
// their own `.changeset/` edits and caches.

import { spawnSync } from "node:child_process";
import {
	cpSync,
	existsSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	readlinkSync,
	realpathSync,
	rmSync,
	statSync,
	symlinkSync,
	unlinkSync,
	writeFileSync,
} from "node:fs";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import process from "node:process";

/// Root of the harness directory, `evals/monochange-skill`.
export const HARNESS_ROOT = resolve(import.meta.dirname, "..");

/// Repository root of the worktree this harness lives in.
///
/// The harness lives at `<repo>/evals/monochange-skill`, so the checkout the
/// fixtures and the graded binary come from is two levels up.
export const REPO_ROOT = resolve(HARNESS_ROOT, "..", "..");

/// Directory holding scenario definitions.
export const SCENARIO_DIR = join(HARNESS_ROOT, "scenarios");

/// Directory holding fixture projects copied per run.
export const FIXTURE_DIR = join(HARNESS_ROOT, "fixtures");

/// Scratch space for run workdirs.
export const WORK_DIR = join(HARNESS_ROOT, ".work");

/// Saved transcripts and machine-readable results.
export const RESULT_DIR = join(HARNESS_ROOT, "results");

/// Skill variants: each directory is a complete `monochange` skill (SKILL.md
/// plus `skills/` and `examples/`). The runner copies the chosen variant into
/// the run workdir as `.claude/skills/monochange`, which takes precedence over
/// any globally installed skill of the same name.
export const SKILL_VARIANT_DIR = join(HARNESS_ROOT, "skill-variants");

/// The live skill package, the source of truth every variant is a copy of.
///
/// `--variant package` installs this directory directly, so the shipping skill
/// can be measured without first freezing a copy under `skill-variants/`.
export const PACKAGE_SKILL_DIR = join(REPO_ROOT, "packages", "monochange__skill");

/// The built-in variant name for `PACKAGE_SKILL_DIR`.
export const PACKAGE_SKILL_VARIANT = "package";

/// The `monochange` CLI the fixtures are graded with.
///
/// The workspace build is authoritative: the harness resolves
/// `<repo>/target/debug/monochange` unless MONOCHANGE_EVAL_CLI_PATH explicitly
/// pins a snapshot of that build. It never falls back to `PATH`. A released
/// `monochange` found on `PATH` may have a different command surface, so
/// grading against it would measure the environment instead of the skill.
export function resolveMonochangeCli(override = process.env["MONOCHANGE_EVAL_CLI_PATH"]): string {
	if (override !== undefined) {
		const path = resolve(override);

		if (!override || !existsSync(path) || !statSync(path).isFile()) {
			throw new Error(`MONOCHANGE_EVAL_CLI_PATH must identify an existing CLI file: ${override}`);
		}

		// Agents and shell checks resolve the command through its directory.
		// A renamed snapshot would silently let another monochange win on PATH.
		if (basename(path) !== "monochange") {
			throw new Error("MONOCHANGE_EVAL_CLI_PATH must retain the executable filename monochange");
		}

		return path;
	}

	const candidate = join(REPO_ROOT, "target", "debug", "monochange");
	if (!existsSync(candidate)) {
		throw new Error(
			`No monochange CLI found. Expected:\n  ${candidate}\n` +
				`Build it from the repository root with: devenv shell -- cargo build -p monochange`,
		);
	}
	return resolve(candidate);
}

/// Resolve a skill variant name to the directory that gets installed.
///
/// Resolution order:
/// 1. an absolute path is used as-is,
/// 2. a directory under `skill-variants/`,
/// 3. the built-in `package` variant, which is `packages/monochange__skill`.
export function resolveSkillVariant(variant: string): string {
	if (variant.startsWith("/")) {
		if (!existsSync(variant)) {
			throw new Error(`Skill variant not found: ${variant}`);
		}
		return variant;
	}

	const frozen = join(SKILL_VARIANT_DIR, variant);
	if (existsSync(frozen)) {
		return frozen;
	}

	if (variant === PACKAGE_SKILL_VARIANT && existsSync(PACKAGE_SKILL_DIR)) {
		return PACKAGE_SKILL_DIR;
	}

	throw new Error(
		`Skill variant not found: ${variant}\n` +
			`Expected a directory under ${SKILL_VARIANT_DIR}, an absolute path, ` +
			`or the built-in "${PACKAGE_SKILL_VARIANT}" variant (${PACKAGE_SKILL_DIR}).`,
	);
}

/// Built-in variants that need no directory under `skill-variants/`.
export function builtinVariants(): string[] {
	return existsSync(PACKAGE_SKILL_DIR) ? [PACKAGE_SKILL_VARIANT] : [];
}

export interface ExecOptions {
	cwd: string;
	/// Seconds before the command is killed.
	timeoutSeconds?: number;
	env?: Record<string, string>;
}

export interface ExecResult {
	exit: number;
	stdout: string;
	stderr: string;
	timedOut: boolean;
}

/// Run a command, capturing combined output without throwing on failure.
export function exec(command: string, options: ExecOptions): ExecResult {
	const result = spawnSync("sh", ["-c", command], {
		cwd: options.cwd,
		encoding: "utf8",
		env: { ...process.env, ...options.env },
		timeout: (options.timeoutSeconds ?? 300) * 1000,
		maxBuffer: 32 * 1024 * 1024,
	});

	const timedOut =
		result.signal === "SIGTERM" ||
		(result.error?.name === "Error" && String(result.error.message).includes("ETIMEDOUT"));

	return {
		exit: result.status ?? (timedOut ? 124 : 1),
		stdout: result.stdout ?? "",
		stderr: `${result.stderr ?? ""}${result.error ? `\n${result.error.message}` : ""}`,
		timedOut,
	};
}

/// Quote one literal argument without allowing shell substitution in paths.
export function quoteShellArgument(value: string): string {
	return `'${value.replaceAll("'", "'\\''")}'`;
}

/// Give mutating graders a copy so saved agent artifacts remain replayable.
export function copyForGrading(workdir: string, destination: string): void {
	const source = realpathSync(workdir);
	const links: { path: string; target: string }[] = [];
	function insideSource(path: string): boolean {
		const offset = relative(source, path);
		return offset !== ".." && !offset.startsWith(`..${sep}`) && !isAbsolute(offset);
	}
	function inspect(directory: string): void {
		for (const entry of readdirSync(directory, { withFileTypes: true })) {
			const path = join(directory, entry.name);
			if (entry.isDirectory()) inspect(path);
			else if (entry.isSymbolicLink()) {
				// Resolve chains and OS aliases such as /tmp -> /private/tmp, then
				// remap only a target inside the saved workspace. Broken links fail.
				const target = realpathSync(resolve(dirname(path), readlinkSync(path)));
				if (!insideSource(target)) {
					throw new Error(`grading symlink escapes saved workspace: ${path}`);
				}
				links.push({ path: relative(source, path), target: relative(source, target) });
			}
		}
	}
	inspect(source);
	resetDir(destination);
	cpSync(source, destination, { recursive: true, verbatimSymlinks: true });
	for (const link of links) {
		const path = join(destination, link.path);
		unlinkSync(path);
		symlinkSync(relative(dirname(path), join(destination, link.target)), path);
	}
}

/// Create an empty directory, removing any previous contents.
export function resetDir(path: string): void {
	rmSync(path, { recursive: true, force: true });
	mkdirSync(path, { recursive: true });
}

/// Copy a fixture tree into a fresh workdir.
///
/// `${MONOCHANGE_ROOT}` in a copied file is rewritten to the repository root so
/// a fixture can reference the local checkout by path. A fixture committed with
/// an absolute path would break for every other checkout.
export function copyFixture(fixtureName: string, destination: string): void {
	const source = join(FIXTURE_DIR, fixtureName);
	if (!existsSync(source)) {
		throw new Error(`Fixture not found: ${source}`);
	}
	resetDir(destination);
	cpSync(source, destination, { recursive: true });
	substitutePlaceholders(destination);
}

/// Rewrite the `${MONOCHANGE_ROOT}` placeholder in every text file under `root`.
function substitutePlaceholders(root: string): void {
	for (const file of walk(root)) {
		if (file.includes(`${sep}target${sep}`) || file.includes(`${sep}.git${sep}`)) {
			continue;
		}
		let contents: string;
		try {
			contents = readFileSync(file, "utf8");
		} catch {
			continue;
		}
		if (!contents.includes("${MONOCHANGE_ROOT}")) {
			continue;
		}
		writeFileSync(file, contents.replaceAll("${MONOCHANGE_ROOT}", REPO_ROOT));
	}
}

/// Recursively list files under `root`.
function walk(root: string): string[] {
	const found: string[] = [];
	for (const entry of readdirSync(root, { withFileTypes: true })) {
		const path = join(root, entry.name);
		if (entry.isDirectory()) {
			found.push(...walk(path));
		} else if (entry.isFile()) {
			found.push(path);
		}
	}
	return found;
}

/// Resolve a path relative to the harness root.
export function harnessPath(...segments: string[]): string {
	return join(HARNESS_ROOT, ...segments);
}

/// Ensure the parent directory of a file exists.
export function ensureParent(path: string): void {
	mkdirSync(dirname(path), { recursive: true });
}
