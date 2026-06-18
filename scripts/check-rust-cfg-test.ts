import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = process.cwd();
const ignoredDirectories = new Set([".git", "target", "worktrees"]);
const violations: string[] = [];

function rustFiles(directory: string): string[] {
	const entries = readdirSync(directory, { withFileTypes: true });
	const files: string[] = [];

	for (const entry of entries) {
		if (entry.isDirectory()) {
			if (!ignoredDirectories.has(entry.name)) {
				files.push(...rustFiles(join(directory, entry.name)));
			}
			continue;
		}

		if (entry.isFile() && entry.name.endsWith(".rs")) {
			files.push(join(directory, entry.name));
		}
	}

	return files;
}

function nextMeaningfulLine(
	lines: string[],
	index: number,
): { line: string; index: number } | undefined {
	for (let current = index + 1; current < lines.length; current += 1) {
		const line = lines[current]?.trim() ?? "";
		if (line.length > 0) {
			return { line, index: current };
		}
	}

	return undefined;
}

function cfgTestIndexes(lines: string[]): number[] {
	const indexes: number[] = [];

	lines.forEach((line, index) => {
		if (line.trim() === "#[cfg(test)]") {
			indexes.push(index);
		}
	});

	return indexes;
}

function validateFile(path: string): void {
	const text = readFileSync(path, "utf8");
	const lines = text.split(/\r?\n/);
	const indexes = cfgTestIndexes(lines);
	const displayPath = relative(root, path);

	if (indexes.length > 1) {
		violations.push(`${displayPath}: expected at most one #[cfg(test)], found ${indexes.length}`);
	}

	for (const index of indexes) {
		let next = nextMeaningfulLine(lines, index);
		while (next?.line.startsWith("#")) {
			next = nextMeaningfulLine(lines, next.index);
		}

		if (!next?.line.startsWith("mod ")) {
			violations.push(
				`${displayPath}:${index + 1}: #[cfg(test)] may only guard a test module declaration`,
			);
		}
	}
}

if (!statSync(root).isDirectory()) {
	throw new Error(`${root} is not a directory`);
}

for (const file of rustFiles(root)) {
	validateFile(file);
}

if (violations.length > 0) {
	console.error("Invalid Rust #[cfg(test)] layout:\n");
	for (const violation of violations) {
		console.error(`- ${violation}`);
	}
	process.exit(1);
}
