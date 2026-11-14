(() => {
	let saved = null;
	try {
		saved = localStorage.getItem("monochange-color-mode");
	} catch {
		// Storage can be unavailable; the system preference still works.
	}
	const dark =
		saved === null
			? window.matchMedia?.("(prefers-color-scheme: dark)").matches === true
			: saved === "dark";
	document.documentElement.className = dark ? "dark" : "light";
})();
