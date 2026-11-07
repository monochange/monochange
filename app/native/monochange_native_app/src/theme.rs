//! Release floor tokens (`--mc-*`) as native Tailwind class strings.
//!
//! Source of truth: Project store `docs/landing-design.md`. Keep aliases in
//! sync with web `:root` / `@theme` so Android and iOS do not fork a purple
//! palette.

/// Page ground — `--mc-page` light mist / dark graphite.
pub const PAGE: &str = "flex-1 bg-[#E8EEF4] text-[#0B141F] dark:bg-[#0B141F] dark:text-[#E8EEF4]";

/// Raised panel — `--mc-surface`.
pub const SURFACE: &str =
	"rounded-[16px] border border-[#C5D0DC] bg-white p-4 dark:border-[#2A3A4D] dark:bg-[#152536]";

/// Primary ink — `--mc-ink` (already on PAGE; use for explicit headings).
pub const INK: &str = "text-[#0B141F] dark:text-[#E8EEF4]";

/// Secondary copy — `--mc-muted`.
pub const MUTED: &str = "text-sm text-[#5A6B7D] dark:text-[#9AADB8]";

/// Hairline rule — `--mc-rule`.
pub const RULE: &str = "border-[#C5D0DC] dark:border-[#2A3A4D]";

/// Primary button — `--mc-accent-deep` fill, 12px control radius, ≥44px touch.
pub const BUTTON_PRIMARY: &str =
	"min-h-[48px] rounded-[12px] bg-[#0F766E] px-4 py-3 font-bold text-white dark:bg-[#0D9488]";

/// Secondary / outline control.
pub const BUTTON_SECONDARY: &str = "min-h-[48px] rounded-[12px] border border-[#5eead4] bg-transparent px-4 py-3 font-bold text-[#0F766E] dark:text-[#2DD4BF]";

/// Soft selected fill — `--mc-accent-soft`.
pub const ACCENT_SOFT: &str = "bg-[#CCFBF1] dark:bg-[#134E4A]";

/// Rare shipping signal — `--mc-signal`.
pub const SIGNAL: &str = "text-[#E4B84A]";

/// Product-floor row gap — `--mc-gap-action`.
pub const GAP_ACTION: &str = "gap-4";
