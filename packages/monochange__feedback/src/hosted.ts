/** Hosted iframe and first-party popup entrypoints; the parent never receives credentials. */
import type { ClientToken } from "./client.ts";
import { mountFeedback, type FeedbackWidget } from "./widget.ts";

interface HostedSession extends ClientToken {
	project: number;
	identity: string;
}

/** Builds links from escaped project slugs, never from a caller-supplied return URL. */
function projectLinks(win: Window) {
	const params = new URL(win.location.href).searchParams;
	const organization = params.get("organization") ?? "";
	const project = params.get("project") ?? "";

	if (!/^[\w.-]{1,100}$/.test(organization) || !/^[\w.-]{1,100}$/.test(project)) {
		throw new Error("Choose a project in the iframe URL.");
	}

	return {
		organization,
		project,
		query: new URLSearchParams({ organization, project }).toString(),
	};
}

/** Rejects malformed popup replies before they reach the transport. */
function validSession(value: HostedSession | undefined): value is HostedSession {
	return (
		!!value &&
		typeof value.token === "string" &&
		value.token.startsWith(`mcfs.${value.project}.`) &&
		Number.isSafeInteger(value.project) &&
		value.project > 0 &&
		typeof value.identity === "string" &&
		!!value.identity &&
		Number.isFinite(value.expires_at) &&
		value.expires_at > Date.now() / 1000
	);
}

/** Owns one frame's popup, credentials, and widget lifecycle. */
class HostedFeedback {
	private readonly status: HTMLElement;
	private readonly signIn: HTMLButtonElement;
	private readonly disconnect: HTMLButtonElement;
	private readonly target: HTMLElement;
	private widget?: FeedbackWidget;
	private session?: HostedSession;
	private popup: Window | null = null;
	private channel = "";
	private query = "";

	constructor(
		private readonly win: Window,
		private readonly mount: typeof mountFeedback,
	) {
		const doc = win.document;
		this.status = doc.querySelector<HTMLElement>("[data-status]")!;
		this.signIn = doc.querySelector<HTMLButtonElement>("[data-sign-in]")!;
		this.disconnect = doc.querySelector<HTMLButtonElement>("[data-disconnect]")!;
		this.target = doc.querySelector<HTMLElement>("[data-feedback]")!;
		const portal = doc.querySelector<HTMLAnchorElement>("[data-portal]")!;

		try {
			const links = projectLinks(win);
			this.query = links.query;
			portal.href = `/p/${encodeURIComponent(links.organization)}/${encodeURIComponent(links.project)}`;
			this.status.textContent = "Sign in to share feedback, vote, and follow progress.";
			this.signIn.addEventListener("click", this.launch);
		} catch (error) {
			this.status.textContent = (error as Error).message;
			this.signIn.disabled = true;
			portal.hidden = true;
		}

		this.disconnect.addEventListener("click", this.clear);
		win.addEventListener("message", this.receive);
		win.addEventListener("pagehide", this.dispose);
	}

	private readonly launch = () => {
		this.popup?.close();
		this.channel = this.win.crypto.randomUUID();
		this.popup = this.win.open(
			`/embed/feedback-auth.html?${this.query}#${this.channel}`,
			"_blank",
			"popup,width=540,height=720",
		);
		this.status.textContent = this.popup
			? "Complete sign-in in the window that opened."
			: "Your browser blocked sign-in. Allow popups or open the feedback portal.";
	};

	private readonly receive = (event: MessageEvent) => {
		if (
			!this.popup ||
			event.source !== this.popup ||
			event.origin !== this.win.location.origin ||
			event.data?.type !== "monochange:feedback-session" ||
			event.data.channel !== this.channel
		)
			return;
		const value = event.data.session as HostedSession | undefined;

		if (!validSession(value)) return;
		if (this.session?.identity !== value.identity) {
			this.widget?.destroy();
			this.widget = undefined;
		}

		this.session = value;
		this.popup.close();
		this.popup = null;
		this.disconnect.hidden = false;
		this.status.textContent = "Signed in. You can share feedback below.";
		this.widget ??= this.mount({
			endpoint: this.win.location.origin,
			project: value.project,
			target: this.target,
			presentation: "inline",
			getClientToken: this.getClientToken,
		});
	};

	private readonly getClientToken = async () => {
		if (!this.session || this.session.expires_at <= Date.now() / 1000 + 30) {
			this.status.textContent =
				"Sign in again to continue. Your draft is kept while you reconnect.";
			throw new Error("Select Sign in to continue.");
		}

		return this.session;
	};

	private readonly clear = () => {
		this.popup?.close();
		this.popup = null;
		this.session = undefined;
		this.widget?.destroy();
		this.widget = undefined;
		this.disconnect.hidden = true;
		this.status.textContent = "Disconnected. Sign in to share feedback.";
	};

	readonly dispose = () => {
		this.clear();
		this.win.removeEventListener("message", this.receive);
		this.win.removeEventListener("pagehide", this.dispose);
		this.signIn.removeEventListener("click", this.launch);
		this.disconnect.removeEventListener("click", this.clear);
	};
}

/** Mounts the hosted form after an explicit first-party sign-in, without third-party cookies. */
export function startHostedFeedback(win: Window, mount = mountFeedback) {
	return new HostedFeedback(win, mount).dispose;
}

/** Exchanges the monochange login cookie only inside a first-party window. */
export function startFeedbackSignIn(win: Window, fetcher = win.fetch.bind(win)) {
	const doc = win.document;
	const status = doc.querySelector<HTMLElement>("[data-status]")!;
	const button = doc.querySelector<HTMLButtonElement>("[data-continue]")!;

	try {
		const links = projectLinks(win);
		const channel = win.location.hash.slice(1);

		if (!/^[\w-]{36}$/.test(channel) || !win.opener || win.parent !== win) {
			throw new Error("Open sign-in from the feedback widget.");
		}

		status.textContent = `Continue to feedback for ${links.organization}/${links.project}. This shares only a feedback session with the widget.`;
		button.addEventListener("click", async () => {
			button.disabled = true;

			try {
				const response = await fetcher("/api/feedback/iframe-session", {
					method: "POST",
					credentials: "same-origin",
					redirect: "error",
					headers: { "Content-Type": "application/json" },
					body: JSON.stringify({ organization: links.organization, project: links.project }),
				});

				if (response.status === 401) {
					const next = `/embed/feedback-auth.html?${links.query}#${channel}`;
					win.location.replace(`/login?next=${encodeURIComponent(next)}`);
					return;
				}

				if (!response.ok)
					throw new Error("Feedback sign-in is unavailable. Please try again later.");
				win.opener.postMessage(
					{ type: "monochange:feedback-session", channel, session: await response.json() },
					win.location.origin,
				);
				status.textContent = "Connected. You can close this window.";
			} catch (error) {
				status.textContent = (error as Error).message;
			} finally {
				button.disabled = false;
			}
		});
	} catch (error) {
		status.textContent = (error as Error).message;
		button.disabled = true;
	}
}
