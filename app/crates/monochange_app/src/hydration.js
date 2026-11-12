(function (root, pkg_path, output_name, wasm_output_name, defer, load = (url) => import(url)) {
	let started;
	let hydrated = false;
	let timer;
	const clicks = [];
	const triggers = ["pointerdown", "touchstart", "keydown", "focusin"];
	const start = () => {
		if (started) return started;
		clearTimeout(timer);
		started = (async () => {
			const mod = await load(`${root}/${pkg_path}/${output_name}.js`);
			await mod.default({ module_or_path: `${root}/${pkg_path}/${wasm_output_name}.wasm` });
			// Capture is installed in the head, before streaming SSR can paint controls.
			// A quick interaction may finish loading WASM before that stream ends.
			if (document.readyState === "loading") {
				await new Promise((resolve) =>
					document.addEventListener("DOMContentLoaded", resolve, { once: true }),
				);
			}
			await mod.hydrate();
			hydrated = true;
			for (const type of triggers) document.removeEventListener(type, start, true);
			document.removeEventListener("click", capture, true);
			for (const [element, event] of clicks) element.dispatchEvent(new MouseEvent("click", event));
			clicks.length = 0;
			document.documentElement.dataset.hydrated = "true";
		})();
		return started;
	};
	const capture = (event) => {
		const button = event.target instanceof Element ? event.target.closest("button") : null;
		// Links, details and form submission already work without WASM.
		if (!hydrated && button && !button.form) {
			event.preventDefault();
			event.stopImmediatePropagation();
			clicks.push([
				button,
				{
					bubbles: true,
					cancelable: true,
					detail: event.detail,
					ctrlKey: event.ctrlKey,
					shiftKey: event.shiftKey,
					altKey: event.altKey,
					metaKey: event.metaKey,
					button: event.button,
				},
			]);
		}
		void start();
	};
	if (!defer) {
		void start();
		return;
	}
	for (const type of triggers)
		document.addEventListener(type, start, { capture: true, passive: true });
	document.addEventListener("click", capture, true);
	const idle = () => {
		timer = setTimeout(() => {
			if ("requestIdleCallback" in window) window.requestIdleCallback(start, { timeout: 1000 });
			else void start();
		}, 4000);
	};
	if (document.readyState === "complete") idle();
	else window.addEventListener("load", idle, { once: true });
});
