import { Session } from "node:inspector";
import { readFileSync } from "node:fs";
import { constants, runInNewContext } from "node:vm";
import { Window } from "happy-dom";
import { afterAll, expect, test, vi } from "vitest";

const coverage = new Session();
coverage.connect();
await new Promise<void>((resolve, reject) =>
	coverage.post("Profiler.enable", (error) => (error ? reject(error) : resolve())),
);
await new Promise<void>((resolve, reject) =>
	coverage.post("Profiler.startPreciseCoverage", { callCount: true, detailed: true }, (error) =>
		error ? reject(error) : resolve(),
	),
);

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
	const start = runInNewContext(
		source,
		{
			window,
			document,
			Element: window.Element,
			MouseEvent: window.MouseEvent,
			console,
			setTimeout: (callback: () => void) => {
				timers.push(callback);
			},
			clearTimeout: vi.fn(),
		},
		{ filename: "monochange-hydration.js" },
	);
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

test("the production import path resolves a real async entry", async () => {
	const window = new Window();
	const start = runInNewContext(
		source,
		{
			window,
			document: window.document,
			Element: window.Element,
			MouseEvent: window.MouseEvent,
			setTimeout,
			clearTimeout,
		},
		{
			filename: "monochange-hydration.js",
			importModuleDynamically: constants.USE_MAIN_CONTEXT_DEFAULT_LOADER,
		},
	);
	start(
		"data:text/javascript,export default async function init(){};export async function hydrate(){};//",
		"",
		"",
		"",
		false,
	);
	await vi.waitFor(() => expect(window.document.documentElement.dataset.hydrated).toBe("true"));
});

test("a loader installed after load still schedules hydration", async () => {
	const window = new Window();
	Object.defineProperty(window.document, "readyState", { value: "complete" });
	const timers: (() => void)[] = [];
	const hydrate = vi.fn(async () => {});
	const start = runInNewContext(
		source,
		{
			window,
			document: window.document,
			Element: window.Element,
			MouseEvent: window.MouseEvent,
			setTimeout: (callback: () => void) => timers.push(callback),
			clearTimeout,
		},
		{ filename: "monochange-hydration.js" },
	);
	start("", "pkg/site", "entry", "entry_bg", true, async () => ({
		default: async () => {},
		hydrate,
	}));
	timers[0]();
	await vi.waitFor(() => expect(hydrate).toHaveBeenCalledOnce());
});

afterAll(async () => {
	const result = await new Promise<import("node:inspector").Profiler.TakePreciseCoverageReturnType>(
		(resolve, reject) =>
			coverage.post("Profiler.takePreciseCoverage", (error, value) =>
				error ? reject(error) : resolve(value),
			),
	);
	coverage.disconnect();
	const scripts = result.result.filter((script) => script.url.endsWith("monochange-hydration.js"));
	const uncovered = new Set<number>();
	for (const script of scripts)
		for (const fn of script.functions)
			for (const range of fn.ranges)
				if (range.count === 0) {
					for (let offset = range.startOffset; offset < range.endOffset; offset++)
						uncovered.add(offset);
				}
	for (const offset of [...uncovered])
		if (
			scripts.some((script) => {
				const ranges = script.functions
					.flatMap((fn) => fn.ranges)
					.filter((range) => range.startOffset <= offset && range.endOffset > offset)
					.sort((a, b) => a.endOffset - a.startOffset - (b.endOffset - b.startOffset));
				return ranges[0]?.count > 0;
			})
		)
			uncovered.delete(offset);
	expect(scripts.length).toBeGreaterThan(0);
	expect([...uncovered].map((offset) => source.slice(offset, offset + 30))).toEqual([]);
	console.log("hydration loader V8 executable ranges: 100% covered");
});
