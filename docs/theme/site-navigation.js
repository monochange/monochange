// Keep the standalone book connected to the application's navigation.
const toolbar = document.querySelector(".left-buttons");
if (toolbar) {
	const home = document.createElement("a");
	home.href = "https://monochange.dev/book";
	home.className = "monochange-site-link";
	home.textContent = "monochange.dev";
	home.setAttribute("aria-label", "Read the book on monochange.dev");
	toolbar.append(home);
}
