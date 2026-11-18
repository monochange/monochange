// Agent invocation for the evaluation harness.
//
// The runner drives a headless coding agent in a throwaway workdir and returns
// its transcript. Two properties matter more than anything else here:
//
// 1. The skill under test must be the only `monochange` skill visible. A
//    globally installed skill of the same name would otherwise silently grade
//    the wrong content, because the agent would read it instead of the variant.
//
// 2. The run must be reproducible. `--setting-sources project` keeps the
//    operator's personal settings, hooks, and plugins out of the run, and
//    `--strict-mcp-config` keeps their MCP servers out.

import { spawn, type ChildProcess } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import process from "node:process";

import { resolveSkillVariant, WORK_DIR } from "./paths.ts";
import type { RunUsage } from "./types.ts";

/// Directory name the skill is installed as inside `<workdir>/.claude/skills`.
export const SKILL_INSTALL_NAME = "monochange";

export interface AgentRunOptions {
	/// Runtime executable or absolute path; useful when devenv provides a restricted PATH.
	agentBin?: string;
	/// Directory the agent works in. It is the project root the agent sees.
	workdir: string;
	/// Skill variant directory name under `skill-variants/`, the built-in
	/// `package` variant, or an absolute path.
	skillVariant: string;
	/// How the skill reaches the agent.
	///
	/// `installed` copies the variant into the workdir as a project skill, the
	/// normal path. `cli` installs nothing: the agent starts with only the
	/// toolkit and has to find the guidance itself, which is the path
	/// `monochange skill` exists to serve.
	skillSource: "installed" | "cli";
	/// The task handed to the agent.
	prompt: string;
	model: string;
	/// Wall-clock limit for the agent.
	timeoutSeconds: number;
	/// Tools the agent may use without prompting.
	allowedTools: string[];
	/// Extra environment variables for the agent process.
	env?: Record<string, string>;
	/// Directory to write `transcript.jsonl` into.
	transcriptDir: string;
	/// Optional system prompt appended to the runtime's default.
	appendSystemPrompt?: string;
	/// Absolute path to the `monochange` CLI that must be first on `PATH`.
	///
	/// An operator's `PATH` commonly holds a released `monochange` whose
	/// command surface differs from the checkout under test. Grading a run that
	/// used the wrong CLI measures the environment, not the skill, so the
	/// workspace binary is always prepended.
	monochangeCli: string;
}

export interface AgentRunResult {
	durationMs?: number;
	/// Flattened assistant text plus tool inputs, used for transcript grading.
	transcript: string;
	/// Submitted Bash command fields only, for checks that grade actual operations.
	commands: string;
	/// Separate tool calls retain shell-variable scope for command grading.
	commandInputs: string[];
	/// Final assistant message only.
	finalMessage: string;
	usage: RunUsage;
	exit: number;
	timedOut: boolean;
	stderr: string;
	/// A runtime-reported failure, including a missing terminal result record.
	error?: string;
	runtimeVersion?: string;
	effectiveModel?: string;
	/// Requested model recorded at execution time; unavailable in legacy saved outcomes.
	requestedModel?: string;
}

/// Prepare skill discovery for a run.
///
/// `installed` copies the variant into the workdir as a project skill —
/// `--setting-sources project` makes the runtime read skills from
/// `<workdir>/.claude/skills/`, so this copy is what the agent actually reads.
/// `cli` installs nothing on purpose: the run then measures whether the CLI
/// alone is enough for an agent to find and follow the guidance.
function prepareSkill(workdir: string, skillVariant: string, source: "installed" | "cli"): void {
	if (source === "cli") {
		// The user-level skill must not leak in through some other discovery
		// path, so an explicit empty project skills directory is the contract.
		const skillsDir = join(workdir, ".claude", "skills");
		rmSync(skillsDir, { recursive: true, force: true });
		mkdirSync(skillsDir, { recursive: true });

		return;
	}

	const variantPath = resolveSkillVariant(skillVariant);

	if (!existsSync(variantPath)) {
		throw new Error(`Skill variant not found: ${variantPath}`);
	}

	const skillsDir = join(workdir, ".claude", "skills");
	rmSync(skillsDir, { recursive: true, force: true });
	mkdirSync(skillsDir, { recursive: true });
	cpSync(variantPath, join(skillsDir, SKILL_INSTALL_NAME), { recursive: true });
}

/// Extract assistant-authored content from a `stream-json` transcript line.
///
/// Only records the assistant actually authored count. Skill bodies, tool
/// results, and file contents the agent merely *read* arrive as other record
/// types; including them would let a check match the skill's own prose instead
/// of the agent's work.
function isRecord(value: unknown): value is Record<string, unknown> {
	return value !== null && typeof value === "object" && !Array.isArray(value);
}

/// Mutable evidence accumulated while reading transcript records.
interface TranscriptState {
	parts: string[];
	commands: string[];
	usage: RunUsage;
	finalMessage: string;
	completed: boolean;
	error: string | undefined;
	runtimeVersion: string | undefined;
	effectiveModel: string | undefined;
}

/// Record the initialization banner that names the runtime and effective model.
function readInitRecord(record: Record<string, unknown>, state: TranscriptState): void {
	const version = record["claude_code_version"];

	if (typeof version === "string") state.runtimeVersion = version;

	const model = record["model"];

	if (typeof model === "string") state.effectiveModel = model;
}

/// Record a tool invocation; only Bash inputs are graded as submitted commands.
function readToolUse(block: Record<string, unknown>, state: TranscriptState): void {
	const input = block["input"];
	const serialized = typeof input === "string" ? input : JSON.stringify(input ?? "");
	const name = typeof block["name"] === "string" ? block["name"] : "";
	state.parts.push(name ? `${name} ${serialized}` : serialized);

	if (name !== "Bash") {
		return;
	}

	if (typeof input === "string") {
		state.commands.push(input);

		return;
	}

	if (isRecord(input) && typeof input["command"] === "string")
		state.commands.push(input["command"]);
}

/// Record one assistant content block, which is either authored text or a tool call.
function readContentBlock(block: unknown, state: TranscriptState, texts: string[]): void {
	if (!isRecord(block)) {
		return;
	}

	if (block["type"] === "text" && typeof block["text"] === "string") {
		state.parts.push(block["text"]);
		texts.push(block["text"]);
	}

	if (block["type"] === "tool_use") readToolUse(block, state);
}

/// Record one assistant turn whose text becomes the latest candidate final message.
function readAssistantRecord(record: Record<string, unknown>, state: TranscriptState): void {
	const message = record["message"];

	if (!isRecord(message)) {
		return;
	}

	const content = message["content"];

	if (!Array.isArray(content)) {
		return;
	}

	state.usage.turns += 1;
	const texts: string[] = [];

	for (const block of content) {
		readContentBlock(block, state, texts);
	}

	state.finalMessage = texts.join("\n");
}

/// Apply the token counts a terminal record reports over the running totals.
function readReportedUsage(reported: unknown, usage: RunUsage): void {
	if (!isRecord(reported)) {
		return;
	}

	for (const [field, key] of [
		["inputTokens", "input_tokens"],
		["outputTokens", "output_tokens"],
		["cacheReadTokens", "cache_read_input_tokens"],
		["cacheCreationTokens", "cache_creation_input_tokens"],
	] as const) {
		const value = reported[key];

		if (typeof value === "number") usage[field] = value;
	}
}

/// Record the terminal result, which finalizes usage and any runtime error.
function readResultRecord(record: Record<string, unknown>, state: TranscriptState): void {
	state.completed = true;
	readReportedUsage(record["usage"], state.usage);

	const result = record["result"];

	if (typeof result === "string") state.finalMessage = result;

	const cost = record["total_cost_usd"];

	if (typeof cost === "number") state.usage.costUsd = cost;

	const turns = record["num_turns"];

	if (typeof turns === "number") state.usage.turns = turns;

	if (record["subtype"] !== "success" || record["is_error"] !== false) {
		state.error = `agent runtime error: ${JSON.stringify(record["errors"] ?? result ?? record["subtype"])}`;
	}
}

/// Dispatch one transcript line; unparseable banners and unknown records carry no evidence.
function readTranscriptLine(line: string, state: TranscriptState): void {
	let record: unknown;

	try {
		record = JSON.parse(line);
	} catch {
		// A runtime banner is not an assistant-authored transcript record.
		return;
	}

	if (!isRecord(record)) {
		return;
	}

	const type = record["type"];

	if (type === "system" && record["subtype"] === "init") {
		readInitRecord(record, state);

		return;
	}

	if (type === "assistant") {
		readAssistantRecord(record, state);

		return;
	}

	if (type === "result") readResultRecord(record, state);
}

/// Parse saved and live transcripts identically; tool results never count as agent evidence.
export function parseAgentTranscript(
	stdout: string,
): Pick<
	AgentRunResult,
	| "transcript"
	| "commands"
	| "commandInputs"
	| "finalMessage"
	| "usage"
	| "error"
	| "runtimeVersion"
	| "effectiveModel"
> {
	const state: TranscriptState = {
		parts: [],
		commands: [],
		usage: {
			inputTokens: 0,
			outputTokens: 0,
			cacheReadTokens: 0,
			cacheCreationTokens: 0,
			costUsd: 0,
			turns: 0,
		},
		finalMessage: "",
		completed: false,
		error: undefined,
		runtimeVersion: undefined,
		effectiveModel: undefined,
	};

	for (const line of stdout.split("\n")) {
		readTranscriptLine(line, state);
	}

	return {
		transcript: state.parts.join("\n"),
		commands: state.commands.join("\n"),
		commandInputs: state.commands,
		finalMessage: state.finalMessage,
		usage: state.usage,
		runtimeVersion: state.runtimeVersion,
		effectiveModel: state.effectiveModel,
		error:
			state.error ??
			(state.completed ? undefined : "agent transcript has no terminal result record"),
	};
}

/// A complete artifact cannot rescue a crashed, interrupted, or runtime-failed agent.
export function agentFailure(
	agent: Pick<AgentRunResult, "exit" | "timedOut" | "error">,
	timeoutSeconds: number,
): string | undefined {
	if (agent.timedOut) {
		return `agent timed out after ${timeoutSeconds}s`;
	}

	if (agent.exit !== 0) {
		return `agent exited ${agent.exit}${agent.error ? `: ${agent.error}` : ""}`;
	}

	return agent.error;
}

/// Regrade the original transcript and execution outcome without launching an agent.
export function readSavedAgent(directory: string): AgentRunResult {
	const outcome: unknown = JSON.parse(readFileSync(join(directory, "outcome.json"), "utf8"));

	if (
		!isRecord(outcome) ||
		typeof outcome["exit"] !== "number" ||
		typeof outcome["timedOut"] !== "boolean"
	) {
		throw new Error(`Invalid saved agent outcome: ${directory}`);
	}

	return {
		...parseAgentTranscript(readFileSync(join(directory, "transcript.jsonl"), "utf8")),
		exit: outcome["exit"],
		timedOut: outcome["timedOut"],
		stderr: readFileSync(join(directory, "stderr.txt"), "utf8"),
		durationMs: typeof outcome["durationMs"] === "number" ? outcome["durationMs"] : undefined,
		requestedModel:
			typeof outcome["requestedModel"] === "string" ? outcome["requestedModel"] : undefined,
	};
}

/// Build the headless runtime arguments; only an appended system prompt is optional.
function agentArguments(options: AgentRunOptions): string[] {
	const args = [
		"-p",
		options.prompt,
		"--model",
		options.model,
		"--output-format",
		"stream-json",
		"--verbose",
		"--permission-mode",
		"bypassPermissions",
		"--setting-sources",
		"project",
		"--strict-mcp-config",
		"--allowedTools",
		...options.allowedTools,
	];

	if (options.appendSystemPrompt) args.push("--append-system-prompt", options.appendSystemPrompt);

	return args;
}

/// Terminate the agent and, on POSIX, every tool process that inherited its group.
function killAgent(child: ChildProcess): void {
	if (process.platform === "win32" || child.pid === undefined) {
		child.kill("SIGKILL");

		return;
	}

	try {
		process.kill(-child.pid, "SIGKILL");
	} catch (error) {
		// The process can exit just before its timer fires.
		if (!(error instanceof Error && "code" in error && error.code === "ESRCH")) {
			throw error;
		}
	}
}

/// Accumulated output of one agent process.
interface AgentStreams {
	stdout: string;
	stderr: string;
}

/// Capture a child's output until it exits; a spawn failure counts as exit 1.
async function collectAgentOutput(
	child: ChildProcess,
): Promise<{ streams: AgentStreams; exit: number }> {
	const streams: AgentStreams = { stdout: "", stderr: "" };

	child.stdout?.on("data", (chunk: Buffer) => {
		streams.stdout += chunk.toString("utf8");
	});
	child.stderr?.on("data", (chunk: Buffer) => {
		streams.stderr += chunk.toString("utf8");
	});

	const exit = await new Promise<number>((resolveExit) => {
		child.on("close", (code) => resolveExit(code ?? 1));
		child.on("error", (error) => {
			streams.stderr += error.message;
			resolveExit(1);
		});
	});

	return { streams, exit };
}

/// Persist the raw streams so a run can be regraded without the agent.
function writeStreams(directory: string, stdout: string, stderr: string): void {
	writeFileSync(join(directory, "transcript.jsonl"), stdout);
	writeFileSync(join(directory, "stderr.txt"), stderr);
}

/// Persist the outcome record read by `readSavedAgent` during a replay.
function writeOutcome(directory: string, result: AgentRunResult): void {
	writeFileSync(
		join(directory, "outcome.json"),
		`${JSON.stringify({ exit: result.exit, timedOut: result.timedOut, error: result.error, durationMs: result.durationMs, requestedModel: result.requestedModel }, null, "\t")}\n`,
	);
}

/// Run one headless agent session and return its transcript.
export async function runAgent(options: AgentRunOptions): Promise<AgentRunResult> {
	const startedAt = Date.now();
	prepareSkill(options.workdir, options.skillVariant, options.skillSource);

	// Transcripts are saved under the results tree; the real HOME is left
	// untouched so the runtime resolves its own credentials normally.
	mkdirSync(options.transcriptDir, { recursive: true });

	const binDir = dirname(options.monochangeCli);
	const child = spawn(options.agentBin ?? "claude", agentArguments(options), {
		cwd: options.workdir,
		env: {
			...process.env,
			...options.env,
			PATH: `${binDir}:${options.env?.["PATH"] ?? process.env["PATH"] ?? ""}`,
		},
		stdio: ["ignore", "pipe", "pipe"],
		// POSIX tool processes inherit this group, so a timeout can terminate
		// every descendant holding the runtime's pipes or editing its workdir.
		detached: process.platform !== "win32",
	});

	let timedOut = false;

	const timer = setTimeout(() => {
		timedOut = true;
		killAgent(child);
	}, options.timeoutSeconds * 1000);

	const { streams, exit } = await collectAgentOutput(child);
	clearTimeout(timer);

	writeStreams(options.transcriptDir, streams.stdout, streams.stderr);

	const result: AgentRunResult = {
		...parseAgentTranscript(streams.stdout),
		exit,
		timedOut,
		stderr: streams.stderr,
		durationMs: Date.now() - startedAt,
		requestedModel: options.model,
	};
	writeOutcome(options.transcriptDir, result);

	return result;
}

/// Include discovery channel and prompt variant in safe artifact directory names.
export function runIdentity(
	scenarioId: string,
	variant: string,
	source: "installed" | "cli",
	repeat: number,
	instructionVariant?: string,
): string {
	const skill = `${encodeURIComponent(variant)}__source-${source}`;
	const instruction = instructionVariant
		? `__instruction-${encodeURIComponent(instructionVariant)}`
		: "";

	return `${encodeURIComponent(scenarioId)}__${skill}__${repeat}${instruction}`;
}

/// Create a per-run scratch directory.
export function runWorkdir(identity: string): string {
	const path = join(WORK_DIR, "runs", identity);
	rmSync(path, { recursive: true, force: true });
	mkdirSync(path, { recursive: true });

	return path;
}
