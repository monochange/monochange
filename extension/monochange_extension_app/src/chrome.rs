//! Extension chrome helpers aligned with the Release floor `--mc-*` tokens.

/// CSS custom properties the popup and options pages must publish on `:root`.
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

/// Class names used by the compact extension chrome (see `assets/app.css`).
pub const CHROME_CLASSES: &[&str] = &[
	"mc-shell",
	"mc-chrome",
	"mc-brand",
	"mc-brand-name",
	"mc-panel",
	"mc-row",
	"mc-button",
	"mc-button-quiet",
	"mc-signal",
	"mc-status",
];

/// Returns whether `css` defines every required `--mc-*` custom property.
#[must_use]
pub fn css_defines_mc_tokens(css: &str) -> bool {
	REQUIRED_MC_TOKENS.iter().all(|token| css.contains(token))
}

/// Light-theme page ground from the design system (cool mist).
pub const MC_PAGE_LIGHT: &str = "#E8EEF4";

/// Interactive accent (teal) for light theme.
pub const MC_ACCENT_LIGHT: &str = "#0D9488";
