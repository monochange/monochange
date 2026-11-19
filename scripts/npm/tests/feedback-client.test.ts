import { describe, expect, it, vi } from "vitest";
import {
	FeedbackClient,
	FeedbackError,
} from "../../../packages/monochange__feedback/src/client.ts";

describe("feedback client", () => {
	it("shares token refresh and retries an expired session once", async () => {
		let rejected = false;
		const getClientToken = vi.fn(async () => ({
			token: "session",
			expires_at: Date.now() / 1000 + 600,
		}));
		const fetcher = vi.fn(async () => {
			if (!rejected) {
				rejected = true;
				return Response.json({ code: "unauthorized", message: "expired" }, { status: 401 });
			}
			return Response.json({ reply: "Choose a request", similar: [], mode: "basic" });
		});
		const client = new FeedbackClient({
			endpoint: "https://monochange.example",
			project: 1,
			getClientToken,
			fetch: fetcher,
		});
		const result = await client.suggest("CSV export");
		expect(result.reply).toBe("Choose a request");
		expect(fetcher).toHaveBeenCalledTimes(2);
		expect(getClientToken).toHaveBeenCalledTimes(2);
		client.destroy();
	});
	it("keeps the submission key stable after an interrupted response", async () => {
		const keys: string[] = [];
		const fetcher = vi.fn(async (_url, init) => {
			keys.push(init.headers["Idempotency-Key"]);
			if (keys.length === 1) throw new Error("connection lost");
			return Response.json({ id: "fb-1", stage: "discussing" });
		});
		const client = new FeedbackClient({
			endpoint: "https://monochange.example",
			project: 1,
			getClientToken: async () => "token",
			fetch: fetcher,
		});
		await expect(client.submit("draft", 1)).rejects.toThrow();
		await expect(client.submit("draft", 1)).resolves.toEqual({ id: "fb-1", stage: "discussing" });
		expect(keys[0]).toBe(keys[1]);
		client.destroy();
	});
	it("a pending token from an old identity cannot complete after reset", async () => {
		let resolve: (value: string) => void;
		const token = new Promise<string>((accept) => {
			resolve = accept;
		});
		const fetcher = vi.fn();
		const client = new FeedbackClient({
			endpoint: "https://monochange.example",
			project: 1,
			getClientToken: () => token,
			fetch: fetcher,
		});
		const pending = client.suggest("CSV export");
		client.reset();
		resolve("old-user-token");
		await expect(pending).rejects.toBeInstanceOf(FeedbackError);
		expect(fetcher).not.toHaveBeenCalled();
		client.destroy();
	});
});

it("reuses an opaque session across status polls instead of exhausting session issuance quotas", async () => {
	const clock = vi.spyOn(Date, "now").mockReturnValue(0);
	const getClientToken = vi.fn(async () => "opaque-session");
	const client = new FeedbackClient({
		endpoint: "https://monochange.example",
		project: 1,
		getClientToken,
		fetch: vi.fn(async () =>
			Response.json({ reply: "Available matches", similar: [], mode: "basic" }),
		),
	});
	try {
		await client.suggest("CSV export");
		clock.mockReturnValue(61_000);
		await client.suggest("CSV export");
		expect(getClientToken).toHaveBeenCalledTimes(1);
	} finally {
		client.destroy();
		clock.mockRestore();
	}
});

describe("feedback protocol", () => {
	const ENDPOINT = "https://monochange.example";
	type Sent = { url: string; method: string; body: unknown; key?: string };
	function recording(answer: (sent: Sent) => Response) {
		const sent: Sent[] = [];
		const fetcher = vi.fn(async (url: string, init: RequestInit) => {
			const request: Sent = {
				url,
				method: init.method ?? "GET",
				body: init.body === undefined ? undefined : JSON.parse(init.body as string),
				key: (init.headers as Record<string, string>)["Idempotency-Key"],
			};
			sent.push(request);
			return answer(request);
		});
		const client = new FeedbackClient({
			endpoint: ENDPOINT,
			project: 7,
			getClientToken: async () => "token",
			getContext: () => ({ route: "/settings?tab=billing#export" }),
			fetch: fetcher as typeof fetch,
		});
		return { client, sent };
	}
	const conversation = {
		id: "draft/1",
		version: 2,
		kind: "feature_request" as const,
		messages: [],
	};

	it("calls each RPC operation with the project in the query and the arguments", async () => {
		const { client, sent } = recording(() => Response.json({}));
		await client.suggest("CSV");
		await client.startConversation("Export CSV");
		await client.getConversation("draft/1");
		await client.sendMessage(conversation, "Monthly");
		await client.submit("draft/1", 2);
		await client.getItem("fb-1");
		await client.reply("fb-1", "Thanks");
		await client.vote("fb-1");
		await client.vote("fb-1", false);
		expect(sent.map(({ url }) => url)).toEqual(
			[
				"suggestions",
				"create_conversation",
				"conversation",
				"conversation_message",
				"submit",
				"item",
				"reply",
				"vote",
				"vote",
			].map((operation) => `${ENDPOINT}/api/feedback_sdk_${operation}?project=7`),
		);
		expect(sent.every(({ method }) => method === "POST")).toBe(true);
		expect(sent.map(({ body }) => body)).toEqual([
			{ project: 7, input: { text: "CSV" } },
			{
				project: 7,
				input: { message: "Export CSV", kind: "feature_request", context: { route: "/settings" } },
			},
			{ project: 7, id: "draft/1" },
			{ project: 7, id: "draft/1", input: { body: "Monthly", version: 2 } },
			{ project: 7, input: { conversation: "draft/1", version: 2 } },
			{ project: 7, id: "fb-1" },
			{ project: 7, id: "fb-1", input: { body: "Thanks" } },
			{ project: 7, id: "fb-1", voted: true },
			{ project: 7, id: "fb-1", voted: false },
		]);
		expect(sent[4].key).toBeTruthy();
		client.destroy();
	});

	it("keeps an RPC failure's code and status", async () => {
		const { client, sent } = recording(() =>
			Response.json(
				{ status: 404, code: "not_found", message: "No such request" },
				{ status: 404 },
			),
		);
		await expect(client.getItem("fb-9")).rejects.toMatchObject({
			code: "not_found",
			status: 404,
			message: "No such request",
		});
		expect(sent).toHaveLength(1);
		client.destroy();
	});

	it("falls back to the REST routes on a site that predates the operations", async () => {
		const { client, sent } = recording(({ url }) =>
			url.includes("/api/feedback_sdk_")
				? new Response("<html>Not found</html>", { status: 404 })
				: Response.json({}),
		);
		await client.suggest("CSV");
		await client.startConversation("Export CSV");
		await client.getConversation("draft/1");
		await client.sendMessage(conversation, "Monthly");
		await client.submit("draft/1", 2);
		await client.getItem("fb-1");
		await client.reply("fb-1", "Thanks");
		await client.vote("fb-1");
		await client.vote("fb-1", false);
		const base = `${ENDPOINT}/api/v1/feedback/7`;
		// Only the first call probes the RPC operation.
		expect(sent.slice(1).map(({ method, url }) => `${method} ${url}`)).toEqual([
			`POST ${base}/suggestions`,
			`POST ${base}/conversations`,
			`GET ${base}/conversations/draft%2F1`,
			`POST ${base}/conversations/draft%2F1/messages`,
			`POST ${base}/items`,
			`GET ${base}/items/fb-1`,
			`POST ${base}/items/fb-1/messages`,
			`PUT ${base}/items/fb-1/vote`,
			`DELETE ${base}/items/fb-1/vote`,
		]);
		expect(sent[1].body).toEqual({ text: "CSV" });
		expect(sent[2].body).toEqual({
			message: "Export CSV",
			kind: "feature_request",
			context: { route: "/settings" },
		});
		expect(sent[5].key).toBeTruthy();
		client.destroy();
	});

	it("falls back when an older site answers a JSON 404 without a code", async () => {
		const { client, sent } = recording(({ url }) =>
			url.includes("/api/feedback_sdk_")
				? Response.json({ error: "missing" }, { status: 404 })
				: Response.json({ reply: "REST", similar: [], mode: "basic" }),
		);
		await expect(client.suggest("CSV")).resolves.toMatchObject({ reply: "REST" });
		expect(sent[1].url).toBe(`${ENDPOINT}/api/v1/feedback/7/suggestions`);
		client.destroy();
	});
});
