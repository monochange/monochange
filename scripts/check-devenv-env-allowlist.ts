import { readdirSync, readFileSync, type Dirent } from "node:fs";
import { join } from "node:path";

// A `devenv shell` step runs after `devenv.yaml`'s `clean` environment has
// wiped every variable that is not on its `keep` allowlist, so a workflow
// `env:` entry or script variable reference missing from that list becomes
// an unbound variable at run time. The v0.17.0 release was stranded by
// exactly this failure: the release post-merge job aborted on
// `RELEASE_COMMIT: unbound variable` before it could dispatch the publish
// workflow. This check fails linting before such a step reaches CI.

interface ParsedStep {
	stepName: string;
	shellIsDevenv: boolean;
	envKeys: string[];
	scriptLines: string[];
}

const ENV_KEY = /^[A-Z][A-Z0-9_]*$/;

function indentOf(line: string): number {
	return line.length - line.trimStart().length;
}

function yamlFiles(directory: string): string[] {
	const files: string[] = [];

	function walk(current: string) {
		let entries: Dirent[];
		try {
			entries = readdirSync(current, { withFileTypes: true });
		} catch {
			return;
		}

		for (const entry of entries) {
			const path = join(current, entry.name);
			if (entry.isDirectory()) {
				walk(path);
				continue;
			}
			if (entry.isFile() && /\.(yml|yaml)$/.test(entry.name)) {
				files.push(path);
			}
		}
	}

	walk(directory);
	return files;
}

function readKeepList(): Set<string> {
	const lines = readFileSync(join(process.cwd(), "devenv.yaml"), "utf8").split("\n");
	const keep = new Set<string>();
	let inClean = false;
	let inKeep = false;

	for (const line of lines) {
		if (line.startsWith("clean:")) {
			inClean = true;
			inKeep = false;
			continue;
		}

		if (inClean && /^\S/.test(line)) {
			inClean = false;
			inKeep = false;
			continue;
		}

		if (inClean && /^ {2}keep:/.test(line)) {
			inKeep = true;
			continue;
		}

		if (inClean && inKeep) {
			const entry = /^ {4}- ([A-Z][A-Z0-9_]*)$/.exec(line);
			if (entry) {
				keep.add(entry[1] as string);
			}
		}
	}

	return keep;
}

// Workflow-level `env:` applies to every step in the file, devenv shells
// included.
function collectFileEnvKeys(lines: string[]): string[] {
	const keys: string[] = [];

	for (let index = 0; index < lines.length; index += 1) {
		const line = lines[index] ?? "";
		if (/^env:\s*$/.test(line)) {
			let cursor = index + 1;
			while (cursor < lines.length) {
				const candidate = lines[cursor] ?? "";
				if (candidate.trim() === "" || indentOf(candidate) < 2) {
					break;
				}
				const key = /^ {2}([A-Z][A-Z0-9_]*):/.exec(candidate);
				if (key) {
					keys.push(key[1] as string);
				}
				cursor += 1;
			}
			break;
		}
		if (/^jobs:\s*$/.test(line)) {
			break;
		}
	}

	return keys;
}

// Job-level `env:` sits above the `steps:` line at the same indent and
// applies to every step in the job.
function collectJobEnvKeys(lines: string[], stepsLine: number, stepsIndent: number): string[] {
	const keys: string[] = [];

	for (let index = stepsLine - 1; index >= 0; index -= 1) {
		const line = lines[index] ?? "";
		if (line.trim() === "") {
			continue;
		}
		if (indentOf(line) < stepsIndent) {
			break;
		}
		if (indentOf(line) === stepsIndent && /^\s*env:\s*$/.test(line)) {
			let cursor = index + 1;
			while (cursor < lines.length) {
				const candidate = lines[cursor] ?? "";
				if (candidate.trim() === "" || indentOf(candidate) < stepsIndent + 2) {
					break;
				}
				const key = new RegExp(`^ {${stepsIndent + 2}}([A-Z][A-Z0-9_]*):`).exec(candidate);
				if (key) {
					keys.push(key[1] as string);
				}
				cursor += 1;
			}
			break;
		}
	}

	return keys;
}

function collectScriptVarReferences(scriptLines: string[]): string[] {
	const script = scriptLines.join("\n");
	const referenced = new Set<string>();
	const defined = new Set<string>();

	for (const match of script.matchAll(/\$\{?([A-Z][A-Z0-9_]*)\}?/g)) {
		referenced.add(match[1] as string);
	}

	// Variables the script defines itself (plain or through `export`) are
	// bound before use and need no allowlist entry.
	for (const match of script.matchAll(/(?:^|[^\w$])(?:export\s+)?([A-Z][A-Z0-9_]*)=/gm)) {
		defined.add(match[1] as string);
	}

	for (const name of defined) {
		referenced.delete(name);
	}

	return [...referenced];
}

function scanStepsSection(lines: string[], stepsLine: number, stepsIndent: number): ParsedStep[] {
	const itemIndent = stepsIndent + 2;
	const keyIndent = stepsIndent + 4;
	const envIndent = stepsIndent + 6;
	const steps: ParsedStep[] = [];

	let index = stepsLine + 1;
	while (index < lines.length) {
		const line = lines[index] ?? "";
		if (line.trim() === "") {
			index += 1;
			continue;
		}
		if (indentOf(line) <= stepsIndent) {
			break;
		}
		if (indentOf(line) !== itemIndent || !line.trimStart().startsWith("- ")) {
			index += 1;
			continue;
		}

		// One step block: from this list item to the next list item at the
		// same indent or any line at or above the steps indent. The first
		// key rides on the `- ` line, so normalize it to the key indent.
		const block: string[] = [`${" ".repeat(keyIndent)}${line.trimStart().slice(2)}`];
		let cursor = index + 1;
		while (cursor < lines.length) {
			const candidate = lines[cursor] ?? "";
			if (candidate.trim() !== "") {
				const candidateIndent = indentOf(candidate);
				if (
					candidateIndent <= stepsIndent ||
					(candidateIndent === itemIndent && candidate.trimStart().startsWith("- "))
				) {
					break;
				}
			}
			block.push(candidate);
			cursor += 1;
		}

		const step: ParsedStep = {
			stepName: "(unnamed step)",
			shellIsDevenv: false,
			envKeys: [],
			scriptLines: [],
		};

		for (let blockIndex = 0; blockIndex < block.length; blockIndex += 1) {
			const blockLine = block[blockIndex] ?? "";
			const keyMatch = new RegExp(`^ {${keyIndent}}([A-Za-z][A-Za-z0-9_-]*):(.*)$`).exec(blockLine);
			if (!keyMatch) {
				continue;
			}

			const key = keyMatch[1] as string;
			const rest = (keyMatch[2] ?? "").trim();

			if (key === "name") {
				step.stepName = rest;
				continue;
			}

			if (key === "shell") {
				if (rest.includes("devenv shell")) {
					step.shellIsDevenv = true;
				}
				continue;
			}

			if (key === "env") {
				for (let envIndex = blockIndex + 1; envIndex < block.length; envIndex += 1) {
					const envLine = block[envIndex] ?? "";
					if (envLine.trim() === "" || indentOf(envLine) < envIndent) {
						break;
					}
					const envKey = new RegExp(`^ {${envIndent}}([A-Z][A-Z0-9_]*):`).exec(envLine);
					if (envKey) {
						step.envKeys.push(envKey[1] as string);
					}
				}
				continue;
			}

			if (key === "run") {
				if (/[|>][+-]?$/.test(rest)) {
					for (let runIndex = blockIndex + 1; runIndex < block.length; runIndex += 1) {
						const runLine = block[runIndex] ?? "";
						if (runLine.trim() === "") {
							step.scriptLines.push(runLine);
							continue;
						}
						if (indentOf(runLine) <= keyIndent) {
							break;
						}
						step.scriptLines.push(runLine);
					}
				} else if (rest !== "") {
					step.scriptLines.push(rest);
				}
			}
		}

		steps.push(step);
		index = cursor;
	}

	return steps;
}

const keep = readKeepList();
const violations: string[] = [];
let devenvStepCount = 0;

for (const path of yamlFiles(join(process.cwd(), ".github"))) {
	const lines = readFileSync(path, "utf8").split("\n");
	const fileEnvKeys = collectFileEnvKeys(lines);

	for (let index = 0; index < lines.length; index += 1) {
		const line = lines[index] ?? "";
		if (!/^\s*steps:\s*$/.test(line)) {
			continue;
		}

		const stepsIndent = indentOf(line);
		const inheritedEnvKeys = [...fileEnvKeys, ...collectJobEnvKeys(lines, index, stepsIndent)];

		for (const step of scanStepsSection(lines, index, stepsIndent)) {
			if (!step.shellIsDevenv) {
				continue;
			}

			devenvStepCount += 1;
			const required = new Set<string>([
				...inheritedEnvKeys,
				...step.envKeys,
				...collectScriptVarReferences(step.scriptLines),
			]);

			for (const name of required) {
				if (ENV_KEY.test(name) && !keep.has(name)) {
					violations.push(
						`${path} step "${step.stepName}" needs ${name} in devenv.yaml clean.keep`,
					);
				}
			}
		}
	}
}

if (violations.length > 0) {
	console.error(
		"devenv shell steps reference environment variables missing from devenv.yaml clean.keep:",
	);
	for (const violation of violations) {
		console.error(`  - ${violation}`);
	}
	process.exit(1);
}

console.log(`devenv shell environment allowlist check passed for ${devenvStepCount} step(s).`);
