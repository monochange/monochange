// Grading for the evaluation harness.
//
// Grades come from artifacts, never from the agent's own summary: a run that
// claims success while leaving versions or changesets inconsistent fails, and a
// run that stays quiet while producing a consistent release plan passes.

import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";

import process from "node:process";

import { exec } from "./paths.ts";
import type { Check, CheckResult } from "./types.ts";

export interface GradeContext {
	/// The workdir the agent operated in.
	workdir: string;
	/// Flattened transcript: assistant text plus tool inputs.
	transcript: string;
	/// Absolute path to the repository's `monochange` CLI.
	monochangeCli: string;
}

/// Run one check against the finished run.
export function runCheck(check: Check, context: GradeContext): CheckResult {
	switch (check.kind) {
		case "command":
			return runCommandCheck(check, context);
		case "file":
			return runFileCheck(check, context);
		case "absent":
			return runAbsentCheck(check, context);
		case "transcript":
			return runTranscriptCheck(check, context);
	}
}

function runCommandCheck(check: Check, context: GradeContext): CheckResult {
	if (!check.command) {
		return { check, passed: false, detail: "check has no command" };
	}
	// `monochange` is shadowed for the whole shell, not prefixed onto it: in
	// `PATH=... a && b` the assignment would apply to `a` alone and `b` would
	// silently resolve to whatever release is on PATH — a stale `monochange`
	// whose command surface differs.
	const binDir = dirname(context.monochangeCli);
	const result = exec(check.command, {
		cwd: context.workdir,
		timeoutSeconds: 300,
		env: { PATH: `${binDir}:${process.env["PATH"] ?? ""}` },
	});
	const expected = check.expectExit ?? 0;
	const output = `${result.stdout}\n${result.stderr}`;

	if (result.timedOut) {
		return { check, passed: false, detail: "command timed out" };
	}
	if (result.exit !== expected) {
		const tail = output.trim().split("\n").slice(-12).join("\n");
		return {
			check,
			passed: false,
			detail: `expected exit ${expected}, got ${result.exit}\n${tail}`,
		};
	}
	if (check.expectOutput && check.expectOutput.length > 0) {
		const missing = check.expectOutput.filter((needle) => !output.includes(needle));
		if (missing.length > 0) {
			const tail = output.trim().split("\n").slice(-12).join("\n");
			return {
				check,
				passed: false,
				detail: `exit ${result.exit} but output is missing: ${missing.join(", ")}\n${tail}`,
			};
		}
	}
	return { check, passed: true, detail: `exit ${result.exit}` };
}

function runFileCheck(check: Check, context: GradeContext): CheckResult {
	if (!check.path) {
		return { check, passed: false, detail: "check has no path" };
	}
	const target = join(context.workdir, check.path);
	if (!existsSync(target)) {
		return { check, passed: false, detail: `${check.path} does not exist` };
	}
	if (!check.match) {
		return { check, passed: true, detail: `${check.path} exists` };
	}
	const contents = readFileSync(target, "utf8");
	const pattern = new RegExp(check.match, "m");
	const found = pattern.test(contents);
	if (check.absent) {
		return found
			? {
					check,
					passed: false,
					detail: `${check.path} matches forbidden /${check.match}/`,
				}
			: { check, passed: true, detail: `${check.path} omits /${check.match}/` };
	}
	if (!found) {
		return {
			check,
			passed: false,
			detail: `${check.path} does not match /${check.match}/`,
		};
	}
	return { check, passed: true, detail: `${check.path} matches` };
}

function runAbsentCheck(check: Check, context: GradeContext): CheckResult {
	if (!check.path) {
		return { check, passed: false, detail: "check has no path" };
	}
	const target = join(context.workdir, check.path);
	if (existsSync(target)) {
		return {
			check,
			passed: false,
			detail: `${check.path} unexpectedly exists`,
		};
	}
	return { check, passed: true, detail: `${check.path} is absent` };
}

function runTranscriptCheck(check: Check, context: GradeContext): CheckResult {
	if (!check.pattern) {
		return { check, passed: false, detail: "check has no pattern" };
	}
	const pattern = new RegExp(check.pattern, "m");
	const found = pattern.test(context.transcript);
	if (check.absent) {
		return found
			? {
					check,
					passed: false,
					detail: `transcript contains forbidden /${check.pattern}/`,
				}
			: { check, passed: true, detail: `transcript omits /${check.pattern}/` };
	}
	return found
		? { check, passed: true, detail: `transcript matches /${check.pattern}/` }
		: {
				check,
				passed: false,
				detail: `transcript does not match /${check.pattern}/`,
			};
}

/// Grade every check, returning results in declaration order.
export function gradeAll(checks: Check[], context: GradeContext): CheckResult[] {
	return checks.map((check) => runCheck(check, context));
}

/// Whether every check passed.
export function allPassed(results: CheckResult[]): boolean {
	return results.every((result) => result.passed);
}
