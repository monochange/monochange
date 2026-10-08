/**
 * Small typed HTTP client shared by the Acme front ends.
 *
 * The client is deliberately transport-only: it builds URLs, merges headers,
 * and decodes JSON without knowing anything about the API's resources.
 */

/** HTTP verbs the client supports. */
export type HttpMethod = "GET" | "POST" | "PUT" | "DELETE";

/** Options accepted by {@link request}. */
export interface RequestOptions {
	/** HTTP verb to use. Defaults to `GET`. */
	method?: HttpMethod;
	/** Headers merged over the client defaults. */
	headers?: Record<string, string>;
	/** JSON-serializable request body. */
	body?: unknown;
	/** Abort signal forwarded to `fetch`. */
	signal?: AbortSignal;
}

/** Response returned by {@link request}. */
export interface ApiResponse<T> {
	/** HTTP status code from the server. */
	status: number;
	/** Decoded JSON payload. */
	data: T;
	/** Response headers keyed by lowercase name. */
	headers: Record<string, string>;
}

const DEFAULT_HEADERS: Record<string, string> = {
	accept: "application/json",
};

/** Join a base URL and path without doubling or dropping the slash. */
export function joinUrl(baseUrl: string, path: string): string {
	return `${baseUrl.replace(/\/+$/, "")}/${path.replace(/^\/+/, "")}`;
}

/** Merge caller headers over the client defaults. */
export function withHeaders(overrides: Record<string, string> = {}): Record<string, string> {
	return { ...DEFAULT_HEADERS, ...overrides };
}

/** Perform a JSON request and return the decoded response. */
export async function request<T>(
	baseUrl: string,
	path: string,
	options: RequestOptions = {},
): Promise<ApiResponse<T>> {
	const response = await fetch(joinUrl(baseUrl, path), {
		method: options.method ?? "GET",
		headers: withHeaders(options.headers),
		body: options.body === undefined ? undefined : JSON.stringify(options.body),
		signal: options.signal,
	});

	if (!response.ok) {
		throw new Error(`${response.status} ${response.statusText}: ${path}`);
	}

	const data = (await response.json()) as T;
	const headers: Record<string, string> = {};
	response.headers.forEach((value, key) => {
		headers[key] = value;
	});

	return { status: response.status, data, headers };
}
