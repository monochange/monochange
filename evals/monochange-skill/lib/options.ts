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

/// Flags whose next argument is their value.
type ValueFlag =
	| "--scenario"
	| "--agent-bin"
	| "--variant"
	| "--repeats"
	| "--timeout"
	| "--model"
	| "--instruction-variant"
	| "--skill-source";

/// Apply a valueless switch; a false return means the flag expects a value.
function applyBooleanFlag(flag: string, options: Options): boolean {
	switch (flag) {
		case "--list":
			options.list = true;
			return true;
		case "--keep":
			options.keep = true;
			return true;
		case "--regrade":
			options.regrade = true;
			return true;
		case "--contract-only":
			options.contractOnly = true;
			return true;
		// Selecting every scenario is the default, so `--all` only exists for symmetry.
		case "--all":
			return true;
		default:
			return false;
	}
}

/// Report whether a flag carries the next argument as its value.
function isValueFlag(flag: string): flag is ValueFlag {
	switch (flag) {
		case "--scenario":
		case "--agent-bin":
		case "--variant":
		case "--repeats":
		case "--timeout":
		case "--model":
		case "--instruction-variant":
		case "--skill-source":
			return true;
		default:
			return false;
	}
}

/// Parse a positive integer option value.
function positiveInteger(flag: string, value: string): number {
	const number = Number(value);

	if (!Number.isSafeInteger(number) || number <= 0) {
		throw new Error(`${flag} must be a positive integer`);
	}

	return number;
}

/// Accept only the two documented skill discovery channels.
function skillSource(value: string): "installed" | "cli" {
	if (value !== "cli" && value !== "installed") {
		throw new Error("--skill-source must be installed or cli");
	}

	return value;
}

/// Apply the value carried by a flag, validating options that have a closed set.
function applyValueFlag(flag: ValueFlag, value: string, options: Options): void {
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
			options.repeats = positiveInteger(flag, value);
			break;
		case "--timeout":
			options.timeoutSeconds = positiveInteger(flag, value);
			break;
		case "--skill-source":
			options.skillSource = skillSource(value);
			break;
	}
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

		if (applyBooleanFlag(flag, options)) {
			continue;
		}

		if (!isValueFlag(flag)) {
			throw new Error(`Unknown option: ${flag}`);
		}

		const value = argv[++index];

		if (!value || value.startsWith("--")) {
			throw new Error(`Missing value for ${flag}`);
		}

		applyValueFlag(flag, value, options);
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
