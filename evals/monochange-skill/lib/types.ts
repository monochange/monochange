// Shared types for the monochange skill evaluation harness.
//
// The harness answers one question: given a realistic monochange release
// planning task, does an agent that has the `monochange` skill reach the
// correct outcome on its own? Every scenario therefore has to be graded from
// artifacts the agent produced, not from how convincing its prose is.

/// One check the grader applies to a finished run.
export type CheckKind =
	/// A shell command must exit with the expected status.
	| "command"
	/// A file must exist, and optionally match a pattern.
	| "file"
	/// A file must not exist.
	| "absent"
	/// The agent transcript (assistant text plus tool inputs) must match.
	| "transcript";

export interface Check {
	/// Short stable id used in reports.
	id: string;
	kind: CheckKind;
	/// Why this check exists, in one sentence. Shown on failure.
	why: string;
	/// Shell command for `command` checks. Run with `sh -c` in the run workdir.
	command?: string;
	/// Expected exit status for `command` checks. Defaults to 0.
	expectExit?: number;
	/// Every substring must appear in stdout+stderr to satisfy the check.
	/// When omitted, only the exit status is graded.
	expectOutput?: string[];
	/// Relative path for `file` and `absent` checks.
	path?: string;
	/// Regex source the file contents must match for `file` checks.
	match?: string;
	/// Regex source for `transcript` checks, applied to the flattened transcript.
	pattern?: string;
	/// Limit transcript checks to submitted Bash commands, excluding explanations and other tool data.
	scope?: "commands";
	/// When true, a `transcript` or `file` check passes if the pattern is *absent*.
	absent?: boolean;
}

export interface Scenario {
	/// Stable id, also the fixture directory name.
	id: string;
	/// One-line summary for reports.
	title: string;
	/// The user-facing request handed to the agent verbatim.
	prompt: string;
	/// Fixture directory under `fixtures/`, copied into the run workdir.
	fixture: string;
	/// Files to write into the workdir after copying the fixture, before the
	/// agent starts. Keys are paths relative to the workdir.
	setupFiles?: Record<string, string>;
	/// Shell commands to run in the workdir before the agent starts.
	setupCommands?: string[];
	/// What a correct solution looks like, for the report.
	expectation: string;
	checks: Check[];
	/// Optional extra prompt used when `--instruction-variant` requests it.
	variants?: Record<string, string>;
	/// Pin how the skill reaches the agent for this scenario. A scenario that
	/// tests CLI-discovered guidance sets `"cli"`; the `--skill-source` flag is
	/// otherwise the default for every scenario.
	skillSource?: "installed" | "cli";
	/// When false, no agent runs: the scenario grades the toolkit itself with
	/// `command` checks, and the transcript is empty. Use it for contracts a
	/// deterministic test pins better than a stochastic agent run.
	agent?: boolean;
}

export interface CommandResult {
	command: string;
	exit: number;
	stdout: string;
	stderr: string;
}

export interface CheckResult {
	check: Check;
	passed: boolean;
	detail: string;
}

export interface RunResult {
	/// Whether this run exercised an agent or only a deterministic CLI contract.
	agent?: boolean;
	provenance?: Provenance;
	runtimeVersion?: string;
	effectiveModel?: string;
	scenario: string;
	title: string;
	/// Skill variant id, or the variant's directory name.
	variant: string;
	/// How the skill reached the agent: installed as a project skill, or only
	/// through the CLI.
	skillSource: "installed" | "cli";
	/// Original requested model, unavailable when legacy execution metadata did not record it.
	model?: string;
	/// Which instruction variant was used, when the scenario defines any.
	instructionVariant?: string;
	repeat: number;
	passed: boolean;
	checks: CheckResult[];
	/// Wall-clock duration of the agent invocation.
	agentDurationMs: number;
	/// Wall-clock duration of grading.
	gradeDurationMs: number;
	/// Path to the saved transcript, relative to the results directory.
	transcriptPath: string;
	/// Tokens and cost reported by the agent runtime, when available.
	usage?: RunUsage;
	/// Populated when the harness itself failed (timeout, crash, missing CLI).
	error?: string;
}

export interface RunUsage {
	inputTokens: number;
	outputTokens: number;
	cacheReadTokens: number;
	cacheCreationTokens: number;
	costUsd: number;
	turns: number;
}

export interface Report {
	mode?: "evaluation" | "regrade";
	/// Current grading checkout and binary (may differ from saved agent provenance during regrade).
	provenance?: Provenance;
	startedAt: string;
	finishedAt: string;
	/// Present only when every agent run records the same original requested model.
	model?: string;
	repeats: number;
	runs: RunResult[];
}

export interface Provenance {
	checkoutCommit: string;
	cliSha256: string;
	/// Omitted for CLI-discovered guidance and agent-free contracts.
	skillSha256?: string;
}
