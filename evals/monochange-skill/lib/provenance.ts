import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { exec, REPO_ROOT } from "./paths.ts";
import type { Provenance } from "./types.ts";

/// Hash the exact executable being evaluated, rather than its display version.
export function fileDigest(path: string): string {
	return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/// Hash a skill tree in stable path order, including filenames and file bytes.
export function skillDigest(root: string): string {
	const hash = createHash("sha256");

	const visit = (relative: string): void => {
		for (const entry of readdirSync(join(root, relative), { withFileTypes: true }).toSorted(
			(a, b) => a.name.localeCompare(b.name, "en"),
		)) {
			const path = join(relative, entry.name);

			if (entry.isDirectory()) {
				visit(path);
			} else if (entry.isFile()) {
				const bytes = readFileSync(join(root, path));
				hash.update(`${path.length}:${path}:${bytes.length}:`).update(bytes);
			} else {
				throw new Error(`Skill tree contains unsupported entry: ${path}`);
			}
		}
	};
	visit("");

	return hash.digest("hex");
}

/// Capture checkout and executable identity once for a report.
export function captureProvenance(monochangeCli: string): Provenance {
	const commit = exec("git rev-parse HEAD", { cwd: REPO_ROOT });

	if (commit.exit !== 0) {
		throw new Error(`Cannot identify evaluated checkout: ${commit.stderr}`);
	}

	return { checkoutCommit: commit.stdout.trim(), cliSha256: fileDigest(monochangeCli) };
}

/// Reject incomplete replay metadata before grading artifacts as attributable evidence.
export function readProvenance(path: string, requireSkillDigest: boolean): Provenance {
	const saved: unknown = JSON.parse(readFileSync(path, "utf8"));

	if (
		saved === null ||
		typeof saved !== "object" ||
		Array.isArray(saved) ||
		!("checkoutCommit" in saved) ||
		typeof saved.checkoutCommit !== "string" ||
		!/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/.test(saved.checkoutCommit) ||
		!("cliSha256" in saved) ||
		typeof saved.cliSha256 !== "string" ||
		!/^[0-9a-f]{64}$/.test(saved.cliSha256)
	) {
		throw new Error(`Invalid saved evaluation provenance: ${path}`);
	}

	const skillSha256 = "skillSha256" in saved ? saved.skillSha256 : undefined;

	if (skillSha256 === undefined) {
		if (requireSkillDigest) {
			throw new Error(`Invalid saved evaluation skill provenance: ${path}`);
		}

		return { checkoutCommit: saved.checkoutCommit, cliSha256: saved.cliSha256 };
	}

	if (typeof skillSha256 !== "string" || !/^[0-9a-f]{64}$/.test(skillSha256)) {
		throw new Error(`Invalid saved evaluation skill provenance: ${path}`);
	}

	return { checkoutCommit: saved.checkoutCommit, cliSha256: saved.cliSha256, skillSha256 };
}
