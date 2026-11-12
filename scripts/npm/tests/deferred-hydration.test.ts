import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { Window } from "happy-dom";
import { expect, test, vi } from "vitest";

const source = readFileSync(
	new URL("../../../app/crates/monochange_app/src/hydration.js", import.meta.url),
	"utf8",
);

function setup(defer = true) {
	const window = new Window();
	const document = window.document;
	document.body.innerHTML =
		'<button id="one"><span>one</span></button><button id="two">two</button><a href="/install">Install</a><details><summary>Open</summary></details><form><button>Submit</button></form>';
	let complete: () => void = () => {};
	const ready = new Promise<void>((resolve) => {
		complete = resolve;
	});
	const hydrate = vi.fn(() => ready);
	const init = vi.fn(async () => {});
	const load = vi.fn(async () => ({ default: init, hydrate }));
	const timers: (() => void)[] = [];
	const idle: (() => void)[] = [];
	const start = runInNewContext(source, {
		window,
		document,
		Element: window.Element,
		MouseEvent: window.MouseEvent,
		console,
		setTimeout: (callback: () => void) => {
			timers.push(callback);
		},
		clearTimeout: vi.fn(),
	});
	Object.defineProperty(window, "requestIdleCallback", {
		configurable: true,
		value: (callback: () => void) => {
			idle.push(callback);
			return 1;
		},
	});
	start("", "pkg/site", "monochange_app", "monochange_app_bg", defer, load);
	return { window, document, load, hydrate, complete, timers, idle };
}

test("eager entries hydrate without waiting for input", async () => {
	const app = setup(false);
	await vi.waitFor(() => expect(app.hydrate).toHaveBeenCalledOnce());
	app.complete();
	await vi.waitFor(() => expect(app.document.documentElement.dataset.hydrated).toBe("true"));
});

test.each(["pointerdown", "touchstart", "keydown", "focusin", "click"])(
	"%s starts once and clicks wait for real completion in order",
	async (type) => {
		const app = setup();
		const applied: string[] = [];
		expect(app.load).not.toHaveBeenCalled();
		app.document.dispatchEvent(new app.window.Event(type, { bubbles: true }));
		app.document
			.querySelector("#one span")
			?.dispatchEvent(new app.window.MouseEvent("click", { bubbles: true, cancelable: true }));
		app.document
			.querySelector("#two")
			?.dispatchEvent(new app.window.MouseEvent("click", { bubbles: true, cancelable: true }));
		await vi.waitFor(() => expect(app.hydrate).toHaveBeenCalledOnce());
		for (const id of ["one", "two"])
			app.document.getElementById(id)?.addEventListener("click", () => applied.push(id));
		expect(applied).toEqual([]);
		app.complete();
		await vi.waitFor(() => expect(applied).toEqual(["one", "two"]));
		app.document
			.getElementById("one")
			?.dispatchEvent(new app.window.MouseEvent("click", { bubbles: true }));
		expect(applied).toEqual(["one", "two", "one"]);
		expect(app.load).toHaveBeenCalledOnce();
	},
);

test("links, summaries, form buttons and non-elements retain native defaults", () => {
	const app = setup();
	for (const target of [
		app.document.querySelector("a"),
		app.document.querySelector("summary"),
		app.document.querySelector("form button"),
		app.document,
	]) {
		const event = new app.window.MouseEvent("click", { bubbles: true, cancelable: true });
		target?.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(false);
	}
	app.complete();
});

test.each([true, false])("fallback after load uses idle when available: %s", async (hasIdle) => {
	const app = setup();
	if (!hasIdle) Reflect.deleteProperty(app.window, "requestIdleCallback");
	app.window.dispatchEvent(new app.window.Event("load"));
	app.timers[0]();
	if (hasIdle) app.idle[0]();
	await vi.waitFor(() => expect(app.hydrate).toHaveBeenCalledOnce());
	app.complete();
});
