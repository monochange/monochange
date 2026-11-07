//! Release floor design tokens and Tailwind class fragments for extension chrome.
//!
//! Colors and lengths mirror `[theme]` in `leptoskit.toml`. The extension
//! entry installs them into leptoskit's `Theme` so `bg-[var(--mc-*)]`
//! utilities resolve (web `:root` sheet + native maps).

use std::collections::BTreeMap;

/// Color tokens published as `--mc-*` (light theme / extension chrome).
pub const THEME_COLORS: &[(&str, &str)] = &[
	("mc-page", "#E8EEF4"),
	("mc-surface", "#FFFFFF"),
	("mc-ink", "#0B141F"),
	("mc-muted", "#5A6B7D"),
	("mc-rule", "#C5D0DC"),
	("mc-accent", "#0D9488"),
	("mc-accent-deep", "#0F766E"),
	("mc-accent-soft", "#CCFBF1"),
	("mc-hero", "#071018"),
	("mc-hero-fg", "#E8EEF4"),
	("mc-hero-muted", "#9AADB8"),
	("mc-signal", "#E4B84A"),
	// ~35% teal wash; theme colors are hex-only (no rgba()).
	("mc-glow", "#0D948859"),
];

/// Length tokens published as `--mc-*`.
pub const THEME_LENGTHS: &[(&str, &str)] = &[
	("mc-radius-strategy", "8px"),
	("mc-radius-nav", "10px"),
	("mc-radius-control", "12px"),
	("mc-radius-panel", "16px"),
	("mc-gap-action", "1rem"),
];

/// CSS custom property names the theme must expose.
pub const REQUIRED_MC_TOKENS: &[&str] = &[
	"--mc-page",
	"--mc-surface",
	"--mc-ink",
	"--mc-muted",
	"--mc-rule",
	"--mc-accent",
	"--mc-accent-deep",
	"--mc-accent-soft",
	"--mc-hero",
	"--mc-hero-fg",
	"--mc-hero-muted",
	"--mc-signal",
	"--mc-glow",
	"--mc-radius-control",
	"--mc-radius-panel",
	"--mc-gap-action",
];

/// Popup shell: mist page ground + compact width.
pub const SHELL: &str =
	"w-[320px] max-w-[380px] min-h-[420px] gap-5 p-5 bg-[var(--mc-page)] text-[var(--mc-ink)]";

/// Sticky brand row under the shell.
pub const CHROME: &str =
	"flex-row items-center justify-between gap-3 pb-3 border-b border-[var(--mc-rule)]";

/// Brand control typography (Bricolage-weight role via extrabold).
pub const BRAND: &str = "font-extrabold text-[1.15rem] tracking-tight text-[var(--mc-ink)]";

/// Secondary chrome label.
pub const EYEBROW: &str = "text-sm font-bold text-[var(--mc-muted)]";

/// Panel title.
pub const TITLE: &str = "text-[1.55rem] font-extrabold tracking-tight text-[var(--mc-ink)]";

/// Supporting copy.
pub const COPY: &str = "text-[0.95rem] text-[var(--mc-muted)] max-w-[34ch]";

/// Raised surface panel.
pub const PANEL: &str = "gap-3 p-4 rounded-[var(--mc-radius-panel)] bg-[var(--mc-surface)] border border-[var(--mc-rule)]";

/// Ruled project row.
pub const ROW: &str =
	"flex-row items-baseline justify-between gap-3 py-2 border-b border-[var(--mc-rule)]";

/// Primary action.
pub const BUTTON: &str = "min-h-12 items-center justify-center px-[1.35rem] py-[0.85rem] rounded-[var(--mc-radius-control)] bg-[var(--mc-accent-deep)] text-white font-bold active:bg-[var(--mc-accent)]";

/// Secondary / quiet action.
pub const BUTTON_QUIET: &str = "min-h-12 items-center justify-center px-[1.35rem] py-[0.85rem] rounded-[var(--mc-radius-control)] bg-[var(--mc-accent-soft)] text-[var(--mc-ink)] font-bold";

/// Action group.
pub const ACTIONS: &str = "flex-row flex-wrap gap-[var(--mc-gap-action)] mt-1";

/// Status line.
pub const STATUS: &str = "text-[0.9rem] text-[var(--mc-muted)]";

/// Live status emphasis.
pub const STATUS_LIVE: &str = "text-[0.9rem] font-bold text-[var(--mc-accent-deep)]";

/// Light-theme page ground from the design system (cool mist).
pub const MC_PAGE_LIGHT: &str = "#E8EEF4";

/// Interactive accent (teal) for light theme.
pub const MC_ACCENT_LIGHT: &str = "#0D9488";

/// Theme maps ready for `Theme::install_maps`.
#[must_use]
pub fn theme_maps() -> (BTreeMap<String, String>, BTreeMap<String, String>) {
	let colors = THEME_COLORS
		.iter()
		.map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
		.collect();
	let lengths = THEME_LENGTHS
		.iter()
		.map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
		.collect();
	(colors, lengths)
}

/// Whether a `:root` / theme CSS dump defines every required `--mc-*` property.
#[must_use]
pub fn css_defines_mc_tokens(css: &str) -> bool {
	REQUIRED_MC_TOKENS.iter().all(|token| css.contains(token))
}

/// Whether `leptoskit.toml` declares the Release floor theme keys.
#[must_use]
pub fn leptoskit_toml_declares_theme(toml: &str) -> bool {
	toml.contains("[theme.colors]")
		&& toml.contains("[theme.lengths]")
		&& THEME_COLORS.iter().all(|(name, value)| {
			toml.contains(name)
				&& toml
					.to_ascii_lowercase()
					.contains(&value.to_ascii_lowercase())
		}) && THEME_LENGTHS.iter().all(|(name, _)| toml.contains(name))
}
