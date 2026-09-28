/**
 * Text normalization helpers shared by the Acme Deno tools.
 *
 * The module is deliberately dependency-light: everything a consumer needs is
 * exported from the package root so the CLI can rely on `workspace:` linkage
 * without pulling npm packages.
 */

import { toCamelCase } from "@std/text";

/** Fold consecutive whitespace into single spaces and trim the ends. */
export function collapseWhitespace(input: string): string {
	return input.replace(/\s+/g, " ").trim();
}

/** Convert an arbitrary heading into a URL-safe slug. */
export function slugify(input: string): string {
	return input
		.toLowerCase()
		.normalize("NFKD")
		.replace(/[\u0300-\u036f]/g, "")
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-+|-+$/g, "");
}

/** Convert a heading into a camelCase identifier safe for generated code. */
export function toIdentifier(input: string): string {
	return toCamelCase(slugify(input).replaceAll("-", " "));
}

/** Truncate text on a word boundary without exceeding `max` characters. */
export function truncate(input: string, max: number): string {
	if (max <= 0) return "";
	if (input.length <= max) return input;
	const sliced = input.slice(0, max + 1);
	const lastSpace = sliced.lastIndexOf(" ");
	const cut = lastSpace > 0 ? sliced.slice(0, lastSpace) : input.slice(0, max);
	return collapseWhitespace(cut.replace(/[\p{P}\p{S}]+$/u, ""));
}
