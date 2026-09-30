// Command-line runner for the monochange skill evaluation harness.
//
// Node 24 runs TypeScript directly (type stripping), so no loader is needed:
//
//   node evals/monochange-skill/run.ts --list
//   node evals/monochange-skill/run.ts --scenario add-changeset --variant package
//   node evals/monochange-skill/run.ts --all --variant package --repeats 3
//
// Results land in `results/` as JSON plus a Markdown scorecard.

import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import process from "node:process";

import { agentFailure, readSavedAgent, runAgent, runIdentity, runWorkdir } from "./lib/agent.ts";
import { allPassed, gradeAll } from "./lib/grade.ts";
import { parseArgs, selectScenarios } from "./lib/options.ts";
import { captureProvenance, readProvenance, skillDigest } from "./lib/provenance.ts";
import { initializeRepository } from "./lib/setup.ts";
import {
	builtinVariants,
	copyFixture,
	copyForGrading,
	ensureParent,
	exec,
	HARNESS_ROOT,
	resolveMonochangeCli,
	resolveSkillVariant,
	quoteShellArgument,
	resetDir,
	RESULT_DIR,
	SCENARIO_DIR,
	SKILL_VARIANT_DIR,
	WORK_DIR,
} from "./lib/paths.ts";
import type { Provenance, Report, RunResult, Scenario } from "./lib/types.ts";

const DEFAULT_TOOLS = ["Read", "Write", "Edit", "Bash", "Glob", "Grep", "Skill", "TodoWrite"];

function loadScenarios(): Scenario[] {
	if (!existsSync(SCENARIO_DIR)) {
		return [];
	}
	const scenarios: Scenario[] = [];
	for (const entry of readdirSync(SCENARIO_DIR).toSorted()) {
		if (!entry.endsWith(".json")) {
			continue;
		}
		const path = join(SCENARIO_DIR, entry);
		const parsed = JSON.parse(readFileSync(path, "utf8")) as Scenario;
		scenarios.push(parsed);
	}
	return scenarios;
}

function loadVariants(): string[] {
	const frozen = existsSync(SKILL_VARIANT_DIR)
		? readdirSync(SKILL_VARIANT_DIR, { withFileTypes: true })
				.filter((entry) => entry.isDirectory())
				.map((entry) => entry.name)
		: [];
	return [...new Set([...frozen, ...builtinVariants()])].toSorted();
}

/// Path to a run workdir, without clearing it when re-grading.
function runWorkdirFor(regrade: boolean, identity: string): string {
	const path = join(WORK_DIR, "runs", identity);
	if (regrade) {
		if (!existsSync(path)) {
			throw new Error(`No saved workdir to re-grade: ${path}`);
		}
		return path;
	}
	return runWorkdir(identity);
}

async function main(): Promise<void> {
	const options = parseArgs(process.argv.slice(2));
	const scenarios = loadScenarios();
	const variants = loadVariants();

	if (options.list) {
		console.log("Scenarios:");
		for (const scenario of scenarios) {
			console.log(`  ${scenario.id.padEnd(28)} ${scenario.title}`);
		}
		console.log("\nSkill variants:");
		for (const variant of variants) {
			console.log(`  ${variant}`);
		}
		return;
	}

	const selectedScenarios = selectScenarios(scenarios, options);
	const selectedVariants =
		options.variants.length > 0 ? options.variants : variants.length > 0 ? variants : ["package"];

	if (selectedScenarios.length === 0) {
		throw new Error("No scenarios selected.");
	}

	if (
		options.instructionVariant &&
		!selectedScenarios.some(
			(scenario) => scenario.variants?.[options.instructionVariant] !== undefined,
		)
	) {
		throw new Error(
			`No selected scenario defines instruction variant: ${options.instructionVariant}`,
		);
	}

	const monochangeCli = resolveMonochangeCli();
	const provenance = captureProvenance(monochangeCli);
	console.log(`monochange CLI: ${monochangeCli}`);
	const version = exec(`${quoteShellArgument(monochangeCli)} --version`, {
		cwd: HARNESS_ROOT,
		timeoutSeconds: 60,
	});
	if (version.exit !== 0) {
		throw new Error(`Cannot run evaluated monochange CLI: ${version.stderr}`);
	}
	console.log(`monochange version: ${version.stdout.trim()}`);

	mkdirSync(RESULT_DIR, { recursive: true });
	const startedAt = new Date().toISOString();
	const runs: RunResult[] = [];

	for (const scenario of selectedScenarios) {
		for (const variant of selectedVariants) {
			for (let repeat = 1; repeat <= options.repeats; repeat += 1) {
				const source = scenario.skillSource ?? options.skillSource;
				const key = `${variant}${source === "cli" ? "-cli" : ""}`;
				const identity = runIdentity(
					scenario.id,
					variant,
					source,
					repeat,
					options.instructionVariant,
				);
				const label = `${scenario.id} / ${key} / run ${repeat}`;
				console.log(`\n=== ${label} ===`);

				try {
					const workdir = runWorkdirFor(options.regrade, identity);
					const transcriptDir = join(RESULT_DIR, "transcripts", identity);
					const provenancePath = join(transcriptDir, "provenance.json");
					const runProvenance: Provenance = options.regrade
						? readProvenance(provenancePath, source === "installed" && scenario.agent !== false)
						: {
								...provenance,
								skillSha256:
									source === "installed" && scenario.agent !== false
										? skillDigest(resolveSkillVariant(variant))
										: undefined,
							};

					if (!options.regrade) {
						// A failed fresh run must not leave an older successful
						// transcript available under the new run's provenance.
						resetDir(transcriptDir);
						writeFileSync(provenancePath, `${JSON.stringify(runProvenance, null, "\t")}\n`);
					}
					if (!options.regrade) {
						copyFixture(scenario.fixture, workdir);

						for (const [path, contents] of Object.entries(scenario.setupFiles ?? {})) {
							const target = join(workdir, path);
							ensureParent(target);
							writeFileSync(target, contents);
						}
						initializeRepository(workdir, scenario.setupCommands ?? []);
						for (const command of scenario.setupCommands ?? []) {
							const result = exec(command, {
								cwd: workdir,
								timeoutSeconds: 300,
								env: { PATH: `${dirname(monochangeCli)}:${process.env["PATH"] ?? ""}` },
							});
							if (result.exit !== 0) {
								throw new Error(
									`setup command failed (${result.exit}): ${command}\n${result.stderr}`,
								);
							}
						}
					}

					const agentStarted = Date.now();
					const agent =
						scenario.agent === false
							? {
									transcript: "",
									commands: "",
									commandInputs: [],
									usage: undefined,
									durationMs: 0,
									runtimeVersion: undefined,
									effectiveModel: undefined,
									requestedModel: undefined,
									timedOut: false,
									exit: 0,
									error: undefined,
								}
							: options.regrade
								? readSavedAgent(transcriptDir)
								: // Runs are sequential to keep resource contention out
									// of the recorded wall-clock duration of each invocation.
									// oxlint-disable-next-line no-await-in-loop
									await runAgent({
										agentBin: options.agentBin,
										workdir,
										skillVariant: variant,
										skillSource: source,
										prompt: options.instructionVariant
											? (scenario.variants?.[options.instructionVariant] ?? scenario.prompt)
											: scenario.prompt,
										model: options.model,
										timeoutSeconds: options.timeoutSeconds,
										allowedTools: DEFAULT_TOOLS,
										monochangeCli,
										transcriptDir,
									});
					const agentDurationMs =
						agent.durationMs ?? (options.regrade ? 0 : Date.now() - agentStarted);

					const gradeStarted = Date.now();
					// Some checks prepare releases or install skills. Preserve the
					// agent's artifacts so a re-grade starts from the same state.
					const gradeWorkdir = join(WORK_DIR, "grades", identity);
					copyForGrading(workdir, gradeWorkdir);
					const checks = gradeAll(scenario.checks, {
						workdir: gradeWorkdir,
						transcript: agent.transcript,
						commands: agent.commands,
						commandInputs: agent.commandInputs,
						monochangeCli,
					});
					const gradeDurationMs = Date.now() - gradeStarted;

					const error = agentFailure(agent, options.timeoutSeconds);
					const passed = !error && allPassed(checks);
					for (const result of checks) {
						const mark = result.passed ? "PASS" : "FAIL";
						console.log(`  [${mark}] ${result.check.id}: ${result.detail.split("\n")[0]}`);
						if (!result.passed) {
							console.log(`         why: ${result.check.why}`);
							const extra = result.detail.split("\n").slice(1).join("\n");
							if (extra.trim()) {
								console.log(
									extra
										.split("\n")
										.map((line) => `         ${line}`)
										.join("\n"),
								);
							}
						}
					}
					console.log(`  => ${passed ? "PASS" : "FAIL"} (${(agentDurationMs / 1000).toFixed(1)}s)`);

					runs.push({
						scenario: scenario.id,
						provenance: runProvenance,
						runtimeVersion: agent.runtimeVersion,
						effectiveModel: agent.effectiveModel,
						title: scenario.title,
						variant,
						skillSource: source,
						model: agent.requestedModel,
						instructionVariant: options.instructionVariant,
						repeat,
						passed,
						checks,
						agentDurationMs,
						gradeDurationMs,
						transcriptPath:
							scenario.agent === false ? "" : join("transcripts", identity, "transcript.jsonl"),
						usage: agent.usage,
						error,
						agent: scenario.agent !== false,
					});
				} catch (error) {
					console.error(`  harness error: ${String(error)}`);
					runs.push({
						scenario: scenario.id,
						title: scenario.title,
						variant,
						skillSource: source,
						model: options.regrade || scenario.agent === false ? undefined : options.model,
						instructionVariant: options.instructionVariant,
						repeat,
						passed: false,
						agent: scenario.agent !== false,
						checks: [],
						agentDurationMs: 0,
						gradeDurationMs: 0,
						transcriptPath: "",
						error: String(error),
					});
				}
			}
		}
	}

	const agentRuns = runs.filter((run) => run.agent);
	const requestedModel = agentRuns[0]?.model;
	const report: Report = {
		mode: options.regrade ? "regrade" : "evaluation",
		provenance,
		startedAt,
		finishedAt: new Date().toISOString(),
		model:
			requestedModel && agentRuns.every((run) => run.model === requestedModel)
				? requestedModel
				: undefined,
		repeats: options.repeats,
		runs,
	};

	const stamp = startedAt.replace(/[:.]/g, "-");
	const jsonPath = join(RESULT_DIR, `report-${stamp}.json`);
	writeFileSync(jsonPath, `${JSON.stringify(report, null, "\t")}\n`);
	writeFileSync(join(RESULT_DIR, "latest.json"), `${JSON.stringify(report, null, "\t")}\n`);
	const markdown = renderMarkdown(report);
	writeFileSync(join(RESULT_DIR, `report-${stamp}.md`), markdown);
	writeFileSync(join(RESULT_DIR, "latest.md"), markdown);

	console.log(`\n${markdown}`);
	console.log(`\nReport: ${jsonPath}`);
	process.exitCode = runs.every((run) => run.passed) ? 0 : 1;
}

function cellKey(run: RunResult): string {
	return `${run.variant} (${run.skillSource}${run.instructionVariant ? `, ${run.instructionVariant}` : ""})`;
}

function renderMarkdown(report: Report): string {
	const lines: string[] = ["# monochange skill evaluation", ""];
	lines.push(
		`- Requested agent model: ${report.model ? `\`${report.model}\`` : "unavailable or mixed (inspect per-run execution metadata)"}`,
	);
	lines.push(`- Mode: ${report.mode ?? "evaluation"}`);
	lines.push(`- Started: ${report.startedAt}`);
	lines.push(`- Finished: ${report.finishedAt}`);
	lines.push(`- Repeats per cell: ${report.repeats}`);
	if (report.provenance) {
		lines.push(`- Grading checkout: \`${report.provenance.checkoutCommit}\``);
		lines.push(`- Grading CLI SHA-256: \`${report.provenance.cliSha256}\``);
	}
	lines.push("");

	const variants = [...new Set(report.runs.map(cellKey))];
	const scenarios = [...new Set(report.runs.map((run) => run.scenario))];

	lines.push("## Pass rate by scenario and skill variant");
	lines.push("");
	lines.push(`| Scenario | ${variants.join(" | ")} |`);
	lines.push(`| --- | ${variants.map(() => "---").join(" | ")} |`);
	for (const scenario of scenarios) {
		const cells = variants.map((variant) => {
			const cellRuns = report.runs.filter(
				(run) => run.scenario === scenario && cellKey(run) === variant,
			);
			if (cellRuns.length === 0) {
				return "-";
			}
			const passed = cellRuns.filter((run) => run.passed).length;
			return `${passed}/${cellRuns.length}`;
		});
		lines.push(`| ${scenario} | ${cells.join(" | ")} |`);
	}
	lines.push("");
	lines.push("Agent-free contracts validate the CLI; they do not compare skill effectiveness.", "");

	const errors = report.runs.filter((run) => run.error);

	if (errors.length > 0) {
		lines.push("## Harness and agent errors", "");
		for (const run of errors) {
			lines.push(`- ${run.scenario} / ${cellKey(run)}: ${run.error?.replaceAll("\n", " ")}`);
		}
		lines.push("");
	}

	lines.push("## Failing checks");
	lines.push("");
	const failures = report.runs.flatMap((run) =>
		run.checks.filter((check) => !check.passed).map((check) => ({ run, check })),
	);
	if (failures.length === 0) {
		lines.push("None.");
	} else {
		lines.push("| Scenario | Variant | Check | Detail |");
		lines.push("| --- | --- | --- | --- |");
		for (const { run, check } of failures) {
			const detail = check.detail.split("\n")[0]?.replaceAll("|", "\\|") ?? "";
			lines.push(`| ${run.scenario} | ${run.variant} | ${check.check.id} | ${detail} |`);
		}
	}
	lines.push("");

	const tokenTotal = report.runs.reduce(
		(sum, run) => sum + (run.usage?.inputTokens ?? 0) + (run.usage?.outputTokens ?? 0),
		0,
	);
	const costTotal = report.runs.reduce((sum, run) => sum + (run.usage?.costUsd ?? 0), 0);
	lines.push("## Cost");
	lines.push("");
	lines.push(`- Runs: ${report.runs.length}`);
	lines.push(`- Input+output tokens: ${tokenTotal}`);
	lines.push(
		`- Recorded agent cost: $${costTotal.toFixed(2)}${report.mode === "regrade" ? " (original invocations; no new agent calls)" : ""}`,
	);
	lines.push("");

	return lines.join("\n");
}

main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
