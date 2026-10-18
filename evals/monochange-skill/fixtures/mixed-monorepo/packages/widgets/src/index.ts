/**
 * Embeddable widgets that call the Acme engine through `@acme/sdk`.
 */

import { createClient, evaluatePath, type ClientOptions } from "@acme/sdk";

/** Render a plain-text result list for a query. */
export async function renderResults(options: ClientOptions, query: string): Promise<string> {
	const client = createClient(options);
	const results = await client.evaluate(query);
	return results.length === 0 ? "No matches" : results.join("\n");
}

/** Build the URL the widget links back to for a query. */
export function resultsUrl(options: ClientOptions, query: string): string {
	return `${options.baseUrl.replace(/\/+$/, "")}${evaluatePath(query)}`;
}
