/** Framework-independent transport shared by browser and React Native integrations. */
export type FeedbackKind = "feature_request" | "bug_report";
export interface FeedbackContext {
	route: string;
	app_version?: string;
	locale?: string;
	element?: { selector: string; label?: string };
}
export interface ClientToken {
	token: string;
	expires_at: number;
}
export interface FeedbackOptions {
	endpoint: string;
	project: number;
	getClientToken: () => Promise<ClientToken | string>;
	getContext?: () => FeedbackContext | undefined;
	fetch?: typeof globalThis.fetch;
}
export interface Suggestion {
	id: string;
	title: string;
	status: string;
	votes: number;
	voted: boolean;
}
export interface AssistantReply {
	reply: string;
	similar: Suggestion[];
	mode: string;
}
export interface Message {
	role: "user" | "assistant";
	body: string;
}
export interface Conversation {
	id: string;
	version: number;
	kind: FeedbackKind;
	messages: Message[];
	item?: string;
}
export interface ConversationReply {
	conversation: Conversation;
	similar: Suggestion[];
	mode: string;
}
export interface Receipt {
	id: string;
	stage: string;
}
export interface Item extends Receipt {
	title: string;
	status: string;
	votes: number;
	voted: boolean;
	messages: { author: string; body: string }[];
	update?: { summary: string; detail: string; links: { label: string; url: string }[] };
}

export class FeedbackError extends Error {
	constructor(
		message: string,
		readonly code = "request_failed",
		readonly status = 0,
		readonly retryAfter?: number,
	) {
		super(message);
		this.name = "FeedbackError";
	}
}

let sequence = 0;
const V1_SESSION_LIFETIME_SECONDS = 600;

/** No browser storage or host identity is sent; the host callback supplies a scoped token. */
export class FeedbackClient {
	readonly base: string;
	private token?: ClientToken;
	private refreshing?: Promise<ClientToken>;
	private epoch = 0;
	private disposed = false;
	private requests = new Set<AbortController>();
	private submissionKeys = new Map<string, string>();
	private fetcher: typeof globalThis.fetch;

	constructor(private readonly options: FeedbackOptions) {
		const url = new URL(options.endpoint);
		if (
			url.protocol !== "https:" &&
			!(url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))
		) {
			throw new FeedbackError("Feedback requires HTTPS", "invalid_configuration");
		}
		if (
			url.username ||
			url.password ||
			url.search ||
			url.hash ||
			!Number.isSafeInteger(options.project) ||
			options.project < 1
		) {
			throw new FeedbackError("Invalid feedback endpoint or project", "invalid_configuration");
		}
		this.base = `${url.href.replace(/\/$/, "")}/api/v1/feedback/${options.project}`;
		this.fetcher = options.fetch ?? globalThis.fetch.bind(globalThis);
	}

	private async session(): Promise<ClientToken> {
		if (this.disposed) throw new FeedbackError("Feedback client was destroyed", "disposed");

		if (this.token && this.token.expires_at > Date.now() / 1000 + 30) return this.token;

		if (this.refreshing) return this.refreshing;
		const epoch = this.epoch;
		const pending = this.options.getClientToken().then((value) => {
			if (epoch !== this.epoch || this.disposed)
				throw new FeedbackError("Feedback identity changed", "identity_changed");
			const token =
				typeof value === "string"
					? { token: value, expires_at: Date.now() / 1000 + V1_SESSION_LIFETIME_SECONDS }
					: value;
			if (
				!token.token ||
				!Number.isFinite(token.expires_at) ||
				token.expires_at <= Date.now() / 1000
			)
				throw new FeedbackError("Host returned an expired feedback session", "invalid_session");
			this.token = token;
			return token;
		});
		this.refreshing = pending;
		try {
			return await pending;
		} finally {
			if (this.refreshing === pending) this.refreshing = undefined;
		}
	}

	private async request<T>(
		path: string,
		method: string,
		body?: unknown,
		key?: string,
		refresh = true,
	): Promise<T> {
		const epoch = this.epoch;
		const session = await this.session();

		if (epoch !== this.epoch)
			throw new FeedbackError("Feedback identity changed", "identity_changed");
		const controller = new AbortController();
		this.requests.add(controller);
		const timer = setTimeout(() => controller.abort(), 20_000);
		try {
			const headers: Record<string, string> = {
				Authorization: `Bearer ${session.token}`,
				"Content-Type": "application/json",
			};

			if (key) headers["Idempotency-Key"] = key;
			const response = await this.fetcher(`${this.base}${path}`, {
				method,
				headers,
				body: body === undefined ? undefined : JSON.stringify(body),
				signal: controller.signal,
				credentials: "omit",
				redirect: "error",
			});

			if (epoch !== this.epoch)
				throw new FeedbackError("Feedback identity changed", "identity_changed");

			if (response.status === 401 && refresh) {
				if (this.token === session) this.token = undefined;

				return this.request<T>(path, method, body, key, false);
			}

			const text = await response.text();

			if (epoch !== this.epoch)
				throw new FeedbackError("Feedback identity changed", "identity_changed");

			if (text.length > 262_144)
				throw new FeedbackError("Feedback response exceeded its size limit", "invalid_response");
			let value;
			try {
				value = text ? JSON.parse(text) : {};
			} catch {
				throw new FeedbackError(
					"Feedback returned an invalid response",
					"invalid_response",
					response.status,
				);
			}

			if (!response.ok) {
				throw new FeedbackError(
					value.message ?? "Feedback request failed",
					value.code,
					response.status,
					Number(response.headers.get("retry-after")) || undefined,
				);
			}

			return value as T;
		} finally {
			clearTimeout(timer);
			this.requests.delete(controller);
		}
	}

	suggest(text: string): Promise<AssistantReply> {
		return this.request("/suggestions", "POST", { text });
	}
	startConversation(
		message: string,
		kind: FeedbackKind = "feature_request",
	): Promise<ConversationReply> {
		const supplied = this.options.getContext?.();
		const context = supplied ? { ...supplied } : undefined;

		if (context) context.route = context.route.split(/[?#]/, 1)[0];

		return this.request("/conversations", "POST", { message, kind, context });
	}
	getConversation(id: string): Promise<{ conversation: Conversation }> {
		return this.request(`/conversations/${encodeURIComponent(id)}`, "GET");
	}
	sendMessage(conversation: Conversation, body: string): Promise<ConversationReply> {
		return this.request(`/conversations/${encodeURIComponent(conversation.id)}/messages`, "POST", {
			body,
			version: conversation.version,
		});
	}
	submit(conversation: string, version: number): Promise<Receipt> {
		const scope = `${conversation}:${version}`;
		let key = this.submissionKeys.get(scope);

		if (!key) {
			key = globalThis.crypto?.randomUUID?.() ?? `request-${Date.now()}-${sequence++}`;
			this.submissionKeys.set(scope, key);
		}

		return this.request("/items", "POST", { conversation, version }, key);
	}
	getItem(id: string): Promise<Item> {
		return this.request(`/items/${encodeURIComponent(id)}`, "GET");
	}
	reply(id: string, body: string): Promise<Receipt> {
		return this.request(`/items/${encodeURIComponent(id)}/messages`, "POST", { body });
	}
	vote(id: string, on = true): Promise<{ id: string; votes: number; voted: boolean }> {
		return this.request(`/items/${encodeURIComponent(id)}/vote`, on ? "PUT" : "DELETE");
	}

	/** Call on logout or account change before requesting another user's token. */
	reset(): void {
		this.epoch += 1;
		this.token = undefined;
		this.refreshing = undefined;
		this.submissionKeys.clear();

		for (const request of this.requests) request.abort();
		this.requests.clear();
	}
	destroy(): void {
		this.reset();
		this.disposed = true;
	}
}
