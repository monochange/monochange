import { exec } from "./paths.ts";

/// Give every fixture its own Git root before setup or agents can discover the parent checkout.
export function initializeRepository(workdir: string, setupCommands: string[]): void {
	const commands = ["git init -b main -q"];

	// Historical fixtures own their initial commit and subsequent release
	// refs. Leave that baseline to their setup instead of committing twice.
	if (!setupCommands.some((command) => /\bgit\s+commit\b/.test(command))) {
		commands.push("git add -A && git commit -S -qm 'chore: initialize evaluation fixture'");
	}

	for (const command of commands) {
		const result = exec(command, { cwd: workdir });

		if (result.exit !== 0) {
			throw new Error(
				`fixture repository setup failed (${result.exit}): ${command}\n${result.stderr}`,
			);
		}
	}
}
