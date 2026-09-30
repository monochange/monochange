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

import { spawn } from "node:child_process";
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
	const parts: string[] = [];
	const commands: string[] = [];
	const usage: RunUsage = {
		inputTokens: 0,
		outputTokens: 0,
		cacheReadTokens: 0,
		cacheCreationTokens: 0,
		costUsd: 0,
		turns: 0,
	};
	let finalMessage = "";
	let completed = false;
	let error: string | undefined;
	let runtimeVersion: string | undefined;
	let effectiveModel: string | undefined;

	for (const line of stdout.split("\n")) {
		let record: unknown;

		try {
			record = JSON.parse(line);
		} catch {
			// A runtime banner is not an assistant-authored transcript record.
			continue;
		}

		if (!isRecord(record)) {
			continue;
		}

		if (record["type"] === "system" && record["subtype"] === "init") {
			if (typeof record["claude_code_version"] === "string")
				runtimeVersion = record["claude_code_version"];
			if (typeof record["model"] === "string") effectiveModel = record["model"];
		}

		if (record["type"] === "assistant" && isRecord(record["message"])) {
			const content = record["message"]["content"];

			if (!Array.isArray(content)) {
				continue;
			}

			usage.turns += 1;
			const texts: string[] = [];

			for (const block of content) {
				if (!isRecord(block)) {
					continue;
				}

				if (block["type"] === "text" && typeof block["text"] === "string") {
					parts.push(block["text"]);
					texts.push(block["text"]);
				}

				if (block["type"] === "tool_use") {
					const input = block["input"];
					const serialized = typeof input === "string" ? input : JSON.stringify(input ?? "");
					const name = typeof block["name"] === "string" ? block["name"] : "";
					parts.push(name ? `${name} ${serialized}` : serialized);

					if (name === "Bash") {
						if (typeof input === "string") commands.push(input);
						else if (isRecord(input) && typeof input["command"] === "string")
							commands.push(input["command"]);
					}
				}
			}

			finalMessage = texts.join("\n");
		}

		if (record["type"] === "result") {
			completed = true;
			const reported = record["usage"];

			if (isRecord(reported)) {
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

			if (typeof record["result"] === "string") finalMessage = record["result"];
			if (typeof record["total_cost_usd"] === "number") usage.costUsd = record["total_cost_usd"];
			if (typeof record["num_turns"] === "number") usage.turns = record["num_turns"];

			if (record["subtype"] !== "success" || record["is_error"] !== false) {
				error = `agent runtime error: ${JSON.stringify(record["errors"] ?? record["result"] ?? record["subtype"])}`;
			}
		}
	}

	return {
		transcript: parts.join("\n"),
		commands: commands.join("\n"),
		commandInputs: commands,
		finalMessage,
		usage,
		runtimeVersion,
		effectiveModel,
		error: error ?? (completed ? undefined : "agent transcript has no terminal result record"),
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

/// Run one headless agent session and return its transcript.
export async function runAgent(options: AgentRunOptions): Promise<AgentRunResult> {
	const startedAt = Date.now();
	prepareSkill(options.workdir, options.skillVariant, options.skillSource);

	// Transcripts are saved under the results tree; the real HOME is left
	// untouched so the runtime resolves its own credentials normally.
	mkdirSync(options.transcriptDir, { recursive: true });

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
	if (options.appendSystemPrompt) {
		args.push("--append-system-prompt", options.appendSystemPrompt);
	}

	const binDir = dirname(options.monochangeCli);
	const child = spawn(options.agentBin ?? "claude", args, {
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

	let stdout = "";
	let stderr = "";
	let timedOut = false;

	const timer = setTimeout(() => {
		timedOut = true;

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
	}, options.timeoutSeconds * 1000);

	child.stdout.on("data", (chunk: Buffer) => {
		stdout += chunk.toString("utf8");
	});
	child.stderr.on("data", (chunk: Buffer) => {
		stderr += chunk.toString("utf8");
	});

	const exit = await new Promise<number>((resolveExit) => {
		child.on("close", (code) => resolveExit(code ?? 1));
		child.on("error", (error) => {
			stderr += error.message;
			resolveExit(1);
		});
	});
	clearTimeout(timer);

	writeFileSync(join(options.transcriptDir, "transcript.jsonl"), stdout);
	writeFileSync(join(options.transcriptDir, "stderr.txt"), stderr);

	const result: AgentRunResult = {
		...parseAgentTranscript(stdout),
		exit,
		timedOut,
		stderr,
		durationMs: Date.now() - startedAt,
		requestedModel: options.model,
	};
	writeFileSync(
		join(options.transcriptDir, "outcome.json"),
		`${JSON.stringify({ exit, timedOut, error: result.error, durationMs: result.durationMs, requestedModel: result.requestedModel }, null, "\t")}\n`,
	);

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
