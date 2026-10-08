/**
 * Thin JavaScript client over the Acme rule engine.
 *
 * The SDK is transport-only so it can run in browsers, Node, and edge workers
 * without polyfills.
 */

/** Options accepted when creating a client. */
export interface ClientOptions {
	/** Base URL of the hosted engine. */
	baseUrl: string;
	/** Optional bearer token sent with every request. */
	token?: string;
}

/** Encode a query for the engine's evaluate endpoint. */
export function evaluatePath(query: string): string {
	return `/evaluate?q=${encodeURIComponent(query)}`;
}

/** Create a client bound to a base URL. */
export function createClient(options: ClientOptions): {
	/** Evaluate a query and return the matching rule keys. */
	evaluate: (query: string) => Promise<string[]>;
} {
	const { baseUrl, token } = options;
	const root = baseUrl.replace(/\/+$/, "");

	return {
		async evaluate(query: string): Promise<string[]> {
			const response = await fetch(`${root}${evaluatePath(query)}`, {
				headers: token ? { authorization: `Bearer ${token}` } : {},
			});

			if (!response.ok) {
				throw new Error(`evaluate failed with status ${response.status}`);
			}

			return (await response.json()) as string[];
		},
	};
}
