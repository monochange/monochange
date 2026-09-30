import process from "node:process";

import type { Scenario } from "./types.ts";

export interface Options {
	scenarios: string[];
	variants: string[];
	repeats: number;
	model: string;
	agentBin: string;
	timeoutSeconds: number;
	list: boolean;
	instructionVariant?: string;
	keep: boolean;
	regrade: boolean;
	contractOnly: boolean;
	skillSource: "installed" | "cli";
}

/// Reject misspelled flags and empty selections before any agent is invoked.
export function parseArgs(argv: string[]): Options {
	const options: Options = {
		scenarios: [],
		variants: [],
		repeats: 1,
		model: process.env["MONOCHANGE_EVAL_MODEL"] ?? "sonnet",
		agentBin: process.env["MONOCHANGE_EVAL_AGENT_BIN"] ?? "claude",
		timeoutSeconds: 900,
		list: false,
		keep: false,
		regrade: false,
		contractOnly: false,
		skillSource: "installed",
	};

	for (let index = 0; index < argv.length; index += 1) {
		const flag = argv[index];

		if (flag === "--") {
			continue;
		}

		if (
			flag === "--list" ||
			flag === "--keep" ||
			flag === "--regrade" ||
			flag === "--contract-only" ||
			flag === "--all"
		) {
			if (flag === "--list") options.list = true;
			if (flag === "--keep") options.keep = true;
			if (flag === "--regrade") options.regrade = true;
			if (flag === "--contract-only") options.contractOnly = true;
			continue;
		}

		if (
			![
				"--scenario",
				"--agent-bin",
				"--variant",
				"--repeats",
				"--timeout",
				"--model",
				"--instruction-variant",
				"--skill-source",
			].includes(flag)
		) {
			throw new Error(`Unknown option: ${flag}`);
		}

		const value = argv[++index];

		if (!value || value.startsWith("--")) {
			throw new Error(`Missing value for ${flag}`);
		}

		switch (flag) {
			case "--scenario":
				options.scenarios.push(value);
				break;
			case "--variant":
				options.variants.push(value);
				break;
			case "--model":
				options.model = value;
				break;
			case "--agent-bin":
				options.agentBin = value;
				break;
			case "--instruction-variant":
				options.instructionVariant = value;
				break;
			case "--repeats":
			case "--timeout": {
				const number = Number(value);

				if (!Number.isSafeInteger(number) || number <= 0) {
					throw new Error(`${flag} must be a positive integer`);
				}

				if (flag === "--repeats") options.repeats = number;
				else options.timeoutSeconds = number;
				break;
			}
			case "--skill-source":
				if (value !== "cli" && value !== "installed") {
					throw new Error("--skill-source must be installed or cli");
				}
				options.skillSource = value;
				break;
		}
	}

	return options;
}

/// Apply scenario filters without silently discarding unknown ids.
export function selectScenarios(scenarios: Scenario[], options: Options): Scenario[] {
	const unknown = options.scenarios.filter(
		(id) => !scenarios.some((scenario) => scenario.id === id),
	);

	if (unknown.length > 0) {
		throw new Error(`Unknown scenarios: ${unknown.join(", ")}`);
	}

	return scenarios.filter(
		(scenario) =>
			(options.scenarios.length === 0 || options.scenarios.includes(scenario.id)) &&
			(!options.contractOnly || scenario.agent === false),
	);
}
