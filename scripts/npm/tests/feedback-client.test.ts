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
