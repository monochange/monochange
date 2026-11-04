// @vitest-environment happy-dom
import { beforeEach, expect, it, vi } from "vitest";
import {
	startFeedbackSignIn,
	startHostedFeedback,
} from "../../../packages/monochange__feedback/src/hosted.ts";
import type { WidgetOptions } from "../../../packages/monochange__feedback/src/widget.ts";

const origin = "https://monochange.example";
const channel = "12345678-1234-1234-1234-123456789abc";

function environment(path = "/embed/feedback.html?organization=acme&project=web") {
	const win = new EventTarget() as unknown as Window;
	const url = new URL(path, origin);
	const popup = { close: vi.fn(), postMessage: vi.fn() } as unknown as Window;
	Object.assign(win, {
		document,
		location: { href: url.href, origin, hash: url.hash, replace: vi.fn() },
		crypto: { randomUUID: () => channel },
		open: vi.fn(() => popup),
		parent: win,
		opener: popup,
	});

	return { win, popup };
}

beforeEach(() => {
	document.body.innerHTML = `<p data-status></p><button data-sign-in></button><button data-disconnect hidden></button><a data-portal></a><main data-feedback></main><button data-continue></button>`;
});

it("trusts only the current same-origin popup and keeps credentials within the frame", async () => {
	const { win, popup } = environment();
	let options: WidgetOptions;
	const widget = { open: vi.fn(), close: vi.fn(), reset: vi.fn(), destroy: vi.fn() };
	const mount = vi.fn((value: WidgetOptions) => {
		options = value;
		return widget;
	});
	const dispose = startHostedFeedback(win, mount);
	const signIn = document.querySelector<HTMLButtonElement>("[data-sign-in]")!;
	signIn.click();
	expect(win.open).toHaveBeenCalledWith(
		`/embed/feedback-auth.html?organization=acme&project=web#${channel}`,
		"_blank",
		"popup,width=540,height=720",
	);
	const session = {
		token: "mcfs.1.secret",
		project: 1,
		identity: "account-1",
		expires_at: Date.now() / 1000 + 600,
	};
	const send = (data: unknown, messageOrigin = origin, source = popup) =>
		win.dispatchEvent(new MessageEvent("message", { origin: messageOrigin, source, data }));
	const data = { type: "monochange:feedback-session", channel, session };
	send(data, "https://attacker.example");
	send(data, origin, win);
	send({ ...data, channel: "wrong" });
	send({ ...data, session: { ...session, expires_at: 0 } });
	send({ ...data, session: { ...session, token: "login-token" } });
	expect(mount).not.toHaveBeenCalled();
	send(data);
	expect(options.presentation).toBe("inline");
	expect(options.project).toBe(1);
	expect(await options.getClientToken()).toEqual(session);
	expect(document.querySelector<HTMLButtonElement>("[data-disconnect]")!.hidden).toBe(false);
	send(data);
	expect(mount).toHaveBeenCalledTimes(1);
	// Refreshing the same account preserves its draft.
	signIn.click();
	send(data);
	expect(mount).toHaveBeenCalledTimes(1);
	// Refresh callbacks never open popups outside a user gesture.
	vi.spyOn(Date, "now").mockReturnValue((session.expires_at + 1) * 1000);
	await expect(options.getClientToken()).rejects.toThrow("Sign in");
	vi.restoreAllMocks();
	signIn.click();
	send({ ...data, session: { ...session, identity: "account-2" } });
	expect(widget.destroy).toHaveBeenCalledTimes(1);
	expect(mount).toHaveBeenCalledTimes(2);
	document.querySelector<HTMLButtonElement>("[data-disconnect]")!.click();
	await expect(options.getClientToken()).rejects.toThrow("Sign in");
	expect(widget.destroy).toHaveBeenCalledTimes(2);
	signIn.click();
	win.dispatchEvent(new Event("pagehide"));
	send(data);
	expect(mount).toHaveBeenCalledTimes(2);
	expect(signIn.onclick).toBeNull();
	dispose();
});

it("offers a portal fallback when popups are blocked and rejects missing projects", () => {
	const { win } = environment();
	vi.mocked(win.open).mockReturnValue(null);
	const dispose = startHostedFeedback(win);
	document.querySelector<HTMLButtonElement>("[data-sign-in]")!.click();
	expect(document.querySelector("[data-status]")!.textContent).toContain("blocked");
	expect(document.querySelector<HTMLAnchorElement>("[data-portal]")!.getAttribute("href")).toBe(
		"/p/acme/web",
	);
	dispose();
	const invalid = environment("/embed/feedback.html");
	startHostedFeedback(invalid.win)();
	expect(document.querySelector<HTMLButtonElement>("[data-sign-in]")!.disabled).toBe(true);
	expect(document.querySelector<HTMLAnchorElement>("[data-portal]")!.hidden).toBe(true);
});

it("signs in first-party, returns only a scoped session, and preserves the popup channel through OAuth", async () => {
	const { win, popup } = environment(
		`/embed/feedback-auth.html?organization=acme&project=web#${channel}`,
	);
	const fetcher = vi
		.fn()
		.mockResolvedValueOnce(new Response("", { status: 401 }))
		.mockResolvedValueOnce(Response.json({ token: "mcfs.1.secret" }));
	startFeedbackSignIn(win, fetcher);
	const button = document.querySelector<HTMLButtonElement>("[data-continue]")!;
	button.click();
	await vi.waitFor(() =>
		expect(win.location.replace).toHaveBeenCalledWith(
			`/login?next=${encodeURIComponent(`/embed/feedback-auth.html?organization=acme&project=web#${channel}`)}`,
		),
	);
	button.click();
	await vi.waitFor(() =>
		expect(popup.postMessage).toHaveBeenCalledWith(
			{ type: "monochange:feedback-session", channel, session: { token: "mcfs.1.secret" } },
			origin,
		),
	);
	expect(fetcher.mock.calls[0][1]).toMatchObject({
		credentials: "same-origin",
		method: "POST",
		body: JSON.stringify({ organization: "acme", project: "web" }),
	});
	fetcher.mockResolvedValueOnce(new Response("", { status: 503 }));
	button.click();
	await vi.waitFor(() =>
		expect(document.querySelector("[data-status]")!.textContent).toContain("unavailable"),
	);
	expect(button.disabled).toBe(false);
});

it("never exchanges cookies or sends a session when the sign-in page is framed or has no opener", () => {
	const { win } = environment(`/embed/feedback-auth.html?organization=acme&project=web#${channel}`);
	const fetcher = vi.fn();
	Object.assign(win, { parent: {} });
	startFeedbackSignIn(win, fetcher);
	expect(document.querySelector<HTMLButtonElement>("[data-continue]")!.disabled).toBe(true);
	expect(fetcher).not.toHaveBeenCalled();
	expect(document.querySelector("[data-status]")!.textContent).toContain("Open sign-in from");
});
