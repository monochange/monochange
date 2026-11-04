import {
	FeedbackClient,
	type Conversation,
	type FeedbackOptions,
	type Item,
	type Receipt,
	type Suggestion,
} from "./client.ts";

export interface WidgetOptions extends FeedbackOptions {
	target?: HTMLElement;
	launcher?: HTMLElement | false;
	theme?: "light" | "dark" | "auto";
	locale?: string;
	styleNonce?: string;
	labels?: Partial<typeof defaultLabels>;
	onSubmitted?: (receipt: Receipt) => void;
}
const defaultLabels = {
	title: "Feedback",
	launcher: "Share feedback",
	close: "Close",
	message: "What would you like to change?",
	feature: "Feature or change",
	bug: "Bug report",
	send: "Find similar requests",
	reply: "Send reply",
	submit: "Submit a new request",
	voted: "Upvoted",
	vote: "Upvote",
	another: "Start another request",
	loading: "Working…",
	sent: "Request submitted",
	placeholder: "Describe the outcome you want, or what went wrong…",
};
export interface FeedbackWidget {
	open(item?: string): void;
	close(): void;
	reset(): void;
	destroy(): void;
}

function node(
	tag: keyof HTMLElementTagNameMap,
	attributes: Record<string, string> = {},
	children: Node[] = [],
) {
	const element = document.createElement(tag);

	for (const [name, value] of Object.entries(attributes)) element.setAttribute(name, value);
	element.append(...children);

	return element;
}

function createWidgetView(options: WidgetOptions) {
	const labels = { ...defaultLabels, ...options.labels };
	const host = document.createElement("div");
	host.dataset.monochangeFeedback = "";
	const root = host.attachShadow({ mode: "open" });
	const style = document.createElement("style");

	if (options.styleNonce) style.nonce = options.styleNonce;
	style.textContent = `
:host{--bg:#fff;--fg:#19152e;--muted:#5b5472;--rule:#d8d2e9;--brand:#4f46e5;font:15px/1.5 system-ui,sans-serif;color:var(--fg);color-scheme:light}
:host([data-theme=dark]){--bg:#241f3b;--fg:#f5f3ff;--muted:#c0b8d8;--rule:#49415f;color-scheme:dark}
*{box-sizing:border-box}button,textarea,select{font:inherit}button{cursor:pointer;border:1px solid var(--rule);border-radius:10px;padding:10px 14px;background:var(--bg);color:var(--fg)}button:disabled{cursor:wait;opacity:.6}button:focus-visible,textarea:focus-visible,select:focus-visible{outline:3px solid #a78bfa;outline-offset:2px}.primary{background:var(--brand);border-color:var(--brand);color:#fff}.launcher{position:fixed;right:20px;bottom:20px;z-index:2147483000;box-shadow:0 6px 24px #0003}
dialog{background:var(--bg);color:var(--fg);border:1px solid var(--rule);border-radius:16px;padding:0;width:min(480px,calc(100vw - 24px));max-height:calc(100dvh - 32px);box-shadow:0 24px 80px #0005}dialog::backdrop{background:#19152e66}header{display:flex;align-items:center;justify-content:space-between;padding:16px 20px;border-bottom:1px solid var(--rule)}h2{font-size:20px;margin:0}.body{padding:20px;overflow:auto;max-height:calc(100dvh - 130px)}label{display:block;font-weight:600;margin-bottom:8px}textarea{width:100%;min-height:110px;resize:vertical;border:1px solid var(--rule);border-radius:10px;padding:12px;color:var(--fg);background:var(--bg)}select{padding:8px;border:1px solid var(--rule);border-radius:8px;color:var(--fg);background:var(--bg);margin-bottom:14px}.actions{display:flex;gap:8px;flex-wrap:wrap;margin-top:12px}.messages p{white-space:pre-wrap;padding:12px;border-radius:10px;background:color-mix(in srgb,var(--brand) 8%,var(--bg));overflow-wrap:anywhere}.messages .user{margin-left:24px;background:color-mix(in srgb,var(--brand) 15%,var(--bg))}.match{border:1px solid var(--rule);border-radius:12px;padding:12px;margin:10px 0}.match p{margin:0 0 8px}small,.status{color:var(--muted)}.error{color:#c23748;white-space:pre-wrap}.hidden{display:none}a{color:var(--brand)}
`;
	const dialog = document.createElement("dialog");
	dialog.setAttribute("aria-labelledby", "feedback-title");

	dialog.lang = options.locale ?? document.documentElement.lang ?? "en";

	dialog.append(
		node("header", {}, [
			node("h2", { id: "feedback-title" }),
			node("button", { type: "button", "data-close": "" }),
		]),
		node("div", { class: "body" }, [
			node("p", { class: "error", role: "alert" }),
			node("p", { class: "status", role: "status", "aria-live": "polite" }),
			node("div", { class: "messages", role: "log", "aria-live": "polite" }),
			node("div", { class: "matches" }),
			node("form", {}, [
				node("label", { for: "feedback-kind" }),
				node("select", { id: "feedback-kind" }, [
					node("option", { value: "feature_request" }),
					node("option", { value: "bug_report" }),
				]),
				node("label", { for: "feedback-message" }),
				node("textarea", { id: "feedback-message", maxlength: "4000", required: "" }),
				node("div", { class: "actions" }, [
					node("button", { class: "primary", type: "submit", "data-send": "" }),
					node("button", { type: "button", "data-submit": "", class: "hidden" }),
					node("button", { type: "button", "data-another": "", class: "hidden" }),
				]),
			]),
		]),
	);
	root.append(style, dialog);
	(options.target ?? document.body).append(host);
	const find = <T extends Element>(selector: string) => dialog.querySelector<T>(selector)!;
	const input = find<HTMLTextAreaElement>("textarea");
	const kind = find<HTMLSelectElement>("select");
	const send = find<HTMLButtonElement>("[data-send]");
	const submit = find<HTMLButtonElement>("[data-submit]");

	const another = find<HTMLButtonElement>("[data-another]");
	const messages = find<HTMLDivElement>(".messages");
	const matches = find<HTMLDivElement>(".matches");
	const status = find<HTMLParagraphElement>(".status");
	const error = find<HTMLParagraphElement>(".error");
	find("h2").textContent = labels.title;
	find("[data-close]").textContent = labels.close;
	find("label[for=feedback-kind]").textContent = labels.title;

	find("label[for=feedback-message]").textContent = labels.message;
	kind.options[0].textContent = labels.feature;
	kind.options[1].textContent = labels.bug;
	input.placeholder = labels.placeholder;
	send.textContent = labels.send;
	submit.textContent = labels.submit;
	another.textContent = labels.another;

	return {
		labels,
		host,
		root,
		dialog,
		input,
		kind,
		send,
		submit,
		another,
		messages,
		matches,
		status,
		error,
	};
}

class WidgetController implements FeedbackWidget {
	private readonly ui: ReturnType<typeof createWidgetView>;
	private readonly client: FeedbackClient;
	private readonly theme = matchMedia("(prefers-color-scheme: dark)");
	private conversation?: Conversation;
	private item?: Item;
	private opened = false;
	private destroyed = false;
	private pending = false;
	private epoch = 0;
	private returnFocus: HTMLElement | null = null;
	private poll?: ReturnType<typeof setInterval>;
	private launcher?: HTMLElement;

	constructor(private readonly options: WidgetOptions) {
		this.client = new FeedbackClient(options);
		this.ui = createWidgetView(options);
		this.theme.addEventListener("change", this.applyTheme);
		this.applyTheme();
		this.bindControls();
		this.createLauncher();
	}

	private readonly applyTheme = () => {
		this.ui.host.dataset.theme =
			this.options.theme === "auto" || !this.options.theme
				? this.theme.matches
					? "dark"
					: "light"
				: this.options.theme;
	};
	private readonly launch = () => this.open();

	private createLauncher() {
		if (this.options.launcher === false) return;
		this.launcher = this.options.launcher;

		if (!this.launcher) {
			this.launcher = document.createElement("button");
			this.launcher.className = "launcher primary";
			this.launcher.textContent = this.ui.labels.launcher;
			this.ui.root.append(this.launcher);
		}

		this.launcher.addEventListener("click", this.launch);
	}

	private bindControls() {
		const { dialog, submit, another } = this.ui;
		dialog.querySelector("form")!.addEventListener("submit", this.sendMessage);
		submit.addEventListener("click", () => void this.run(() => this.submitDraft()));
		another.addEventListener("click", () => this.reset());
		dialog.querySelector("[data-close]")!.addEventListener("click", () => this.close());
		dialog.addEventListener("cancel", (event) => {
			event.preventDefault();
			this.close();
		});
		dialog.addEventListener("click", (event) => {
			if (event.target !== dialog) return;
			const rect = dialog.getBoundingClientRect();
			if (
				event.clientX < rect.left ||
				event.clientX > rect.right ||
				event.clientY < rect.top ||
				event.clientY > rect.bottom
			)
				this.close();
		});
	}

	private renderMessages(values: { body: string; role?: string; author?: string }[]) {
		this.ui.messages.replaceChildren(
			...values.map((message) => {
				const paragraph = document.createElement("p");
				paragraph.textContent = message.body;
				paragraph.className =
					message.role === "user" || message.author === "submitter" ? "user" : "assistant";
				return paragraph;
			}),
		);
	}

	private renderMatches(values: Suggestion[]) {
		this.ui.matches.replaceChildren(
			...values.map((suggestion) => this.matchCard(suggestion, values)),
		);
	}

	private matchCard(suggestion: Suggestion, values: Suggestion[]) {
		const card = document.createElement("article");
		card.className = "match";
		const title = document.createElement("p");
		title.textContent = suggestion.title;
		const metadata = document.createElement("small");
		metadata.textContent = `${suggestion.status.replaceAll("_", " ")} · ${suggestion.votes}`;
		const vote = document.createElement("button");
		vote.type = "button";

		vote.textContent = suggestion.voted ? this.ui.labels.voted : this.ui.labels.vote;
		vote.setAttribute("aria-pressed", String(suggestion.voted));
		vote.addEventListener(
			"click",
			() =>
				void this.run(async () => {
					const result = await this.client.vote(suggestion.id, !suggestion.voted);
					suggestion.voted = result.voted;
					suggestion.votes = result.votes;
					this.renderMatches(values);
				}),
		);
		card.append(title, metadata, document.createElement("br"), vote);

		return card;
	}

	private controls() {
		const { dialog, input, kind, send, submit, another, labels } = this.ui;

		for (const button of dialog.querySelectorAll<HTMLButtonElement>(".body button"))
			button.disabled = this.pending;
		input.disabled = this.pending;
		kind.disabled = this.pending || !!this.conversation || !!this.item;
		send.textContent = this.pending
			? labels.loading
			: this.conversation || this.item
				? labels.reply
				: labels.send;
		submit.classList.toggle("hidden", !this.conversation || !!this.item);
		another.classList.toggle("hidden", !this.item);
	}

	private async run(action: () => Promise<void>) {
		if (this.pending || this.destroyed) return;
		const version = this.epoch;
		this.pending = true;
		this.ui.error.textContent = "";
		this.controls();
		try {
			await action();
		} catch (failure) {
			if (version === this.epoch)
				this.ui.error.textContent =
					failure instanceof Error ? failure.message : "Feedback could not be sent. Please retry.";
		} finally {
			if (version === this.epoch) {
				this.pending = false;
				this.controls();
			}
		}
	}

	private async refreshItem(id: string) {
		const version = this.epoch;
		const latest = await this.client.getItem(id);

		if (version !== this.epoch || this.destroyed) return;
		this.item = latest;
		this.ui.status.textContent = `${latest.title} · ${latest.status}`;
		this.renderMessages(latest.messages);
		this.controls();
	}

	private polling() {
		if (this.poll) clearInterval(this.poll);
		this.poll = setInterval(() => {
			if (this.opened && this.item && !this.pending)
				void this.run(() => this.refreshItem(this.item!.id));
		}, 30_000);
	}

	private readonly sendMessage = (event: SubmitEvent) => {
		event.preventDefault();
		const body = this.ui.input.value.trim();
		if (!body) return;
		void this.run(async () => {
			if (this.item) {
				await this.client.reply(this.item.id, body);
				await this.refreshItem(this.item.id);
			} else {
				const result = this.conversation
					? await this.client.sendMessage(this.conversation, body)
					: await this.client.startConversation(
							body,
							this.ui.kind.value as "feature_request" | "bug_report",
						);
				this.conversation = result.conversation;
				this.renderMessages(this.conversation.messages);
				this.renderMatches(result.similar);
			}

			this.ui.input.value = "";
		});
	};

	private async submitDraft() {
		if (!this.conversation) return;
		const receipt = await this.client.submit(this.conversation.id, this.conversation.version);
		this.ui.status.textContent = this.ui.labels.sent;
		this.ui.matches.replaceChildren();
		await this.refreshItem(receipt.id);
		this.polling();
		this.options.onSubmitted?.(receipt);
	}

	open(id?: string) {
		if (this.destroyed) return;
		this.returnFocus = document.activeElement as HTMLElement;

		if (!this.opened) this.ui.dialog.showModal();
		this.opened = true;
		this.ui.input.focus();

		if (id) void this.run(() => this.refreshItem(id));
		this.polling();
	}
	close() {
		this.opened = false;
		this.ui.dialog.close();
		this.returnFocus?.focus();

		if (this.poll) clearInterval(this.poll);
	}
	reset() {
		this.epoch += 1;
		this.client.reset();
		this.conversation = undefined;
		this.item = undefined;
		this.pending = false;
		this.ui.input.value = "";
		this.ui.messages.replaceChildren();
		this.ui.matches.replaceChildren();

		this.ui.status.textContent = "";
		this.ui.error.textContent = "";

		if (this.poll) clearInterval(this.poll);
		this.controls();
	}
	destroy() {
		if (this.destroyed) return;
		this.close();
		this.reset();
		this.destroyed = true;
		this.client.destroy();
		this.launcher?.removeEventListener("click", this.launch);
		this.theme.removeEventListener("change", this.applyTheme);
		this.ui.host.remove();
	}
}

/** Mounts an isolated, keyboard-accessible widget; user content is always text. */
export function mountFeedback(options: WidgetOptions): FeedbackWidget {
	return new WidgetController(options);
}
