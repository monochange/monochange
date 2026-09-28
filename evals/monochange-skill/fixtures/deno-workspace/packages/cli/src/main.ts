/**
 * Command line entrypoint for the Acme text tools.
 *
 * Run from the workspace root with:
 *
 * ```sh
 * deno task start --cwd packages/cli slug "Release planning for monorepos"
 * ```
 */

import { collapseWhitespace, slugify, toIdentifier, truncate } from "@acme/shared";

const USAGE = `usage: deno run src/main.ts <command> [text]

commands:
  slug <text>         URL-safe slug
  collapse <text>     folded whitespace
  identifier <text>   camelCase identifier
  truncate <text>     truncate to 40 characters on a word boundary
`;

function main(): number {
	const [command, ...rest] = Deno.args;
	const text = collapseWhitespace(rest.join(" "));

	if (command === undefined || text === "") {
		console.error(USAGE);
		return 2;
	}

	switch (command) {
		case "slug":
			console.log(slugify(text));
			return 0;
		case "collapse":
			console.log(collapseWhitespace(text));
			return 0;
		case "identifier":
			console.log(toIdentifier(text));
			return 0;
		case "truncate":
			console.log(truncate(text, 40));
			return 0;
		default:
			console.error(`unknown command: ${command}`);
			console.error(USAGE);
			return 2;
	}
}

if (import.meta.main) {
	Deno.exit(main());
}
