// @vitest-environment happy-dom
import { expect, it, vi } from "vitest";
import { mountFeedback } from "../../../packages/monochange__feedback/src/widget.ts";

it("renders user content as text, offers explicit votes, and resets its draft", async () => {
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
	HTMLDialogElement.prototype.close = function () {
		this.open = false;
	};
	const fetcher = vi.fn(async (url: string, options: RequestInit) => {
		if (url.endsWith("/vote")) return Response.json({ id: "fb-1", votes: 2, voted: true });
		expect(JSON.parse(options.body as string).message).toBe("<script>alert('test')</script>");
		return Response.json({
			conversation: {
				id: "draft",
				version: 1,
				messages: [
					{ role: "user", body: "<script>alert('test')</script>" },
					{ role: "assistant", body: "Which outcome?" },
				],
			},
			similar: [
				{ id: "fb-1", title: "Export CSV", status: "under_review", votes: 1, voted: false },
			],
			mode: "basic",
		});
	});
	const widget = mountFeedback({
		endpoint: "https://feedback.example",
		project: 1,
		getClientToken: async () => "session",
		fetch: fetcher as typeof fetch,
		launcher: false,
		styleNonce: "host-csp-nonce",
	});
	widget.open();
	const root = document.querySelector("[data-monochange-feedback]")!.shadowRoot!;
	expect(root.querySelector("style")!.nonce).toBe("host-csp-nonce");
	const input = root.querySelector("textarea")!;
	input.value = "<script>alert('test')</script>";
	root
		.querySelector("form")!
		.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
	await vi.waitFor(() => expect(root.textContent).toContain("Which outcome?"));
	expect(root.querySelector("script")).toBeNull();
	expect(root.querySelector<HTMLButtonElement>("[data-submit]")!.classList.contains("hidden")).toBe(
		false,
	);
	root.querySelector<HTMLButtonElement>(".match button")!.click();
	await vi.waitFor(() => expect(root.querySelector(".match button")!.textContent).toBe("Upvoted"));
	widget.reset();
	expect(root.querySelector(".messages")!.textContent).toBe("");
	expect(input.value).toBe("");
	widget.destroy();
	expect(document.querySelector("[data-monochange-feedback]")).toBeNull();
});

it("embeds inline without a launcher, modal backdrop, or automatic focus", () => {
	HTMLDialogElement.prototype.show = vi.fn(function () {
		this.open = true;
	});
	HTMLDialogElement.prototype.showModal = vi.fn();
	HTMLDialogElement.prototype.close = function () {
		this.open = false;
	};
	const control = document.createElement("button");
	document.body.append(control);
	control.focus();
	const widget = mountFeedback({
		endpoint: "https://feedback.example",
		project: 1,
		getClientToken: async () => "session",
		presentation: "inline",
	});
	const host = document.querySelector<HTMLElement>("[data-monochange-feedback]")!;
	const root = host.shadowRoot!;
	expect(host.dataset.presentation).toBe("inline");
	expect(root.querySelector("dialog")!.open).toBe(true);
	expect(root.querySelector(".launcher")).toBeNull();
	expect(root.querySelector<HTMLButtonElement>("[data-close]")!.hidden).toBe(true);
	expect(document.activeElement).toBe(control);
	expect(HTMLDialogElement.prototype.showModal).not.toHaveBeenCalled();
	widget.close();
	expect(root.querySelector("dialog")!.open).toBe(false);
	widget.open();
	expect(root.querySelector("dialog")!.open).toBe(true);
	expect(HTMLDialogElement.prototype.showModal).not.toHaveBeenCalled();
	widget.destroy();
	control.remove();
});
