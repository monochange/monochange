//! Release floor palette as **Tailwind utilities** for leptoskit guests.
//!
//! Matches `docs/landing-design.md` brand scale (teal) and ink/mist neutrals.
//! Prefer these utility strings in `class=` — do not invent semantic CSS
//! class names (`.mc-page`, `.button-brand`, …) for native/shared Element trees.
//!
//! | Design token        | Tailwind utilities                                      |
//! |---------------------|---------------------------------------------------------|
//! | `--mc-page`         | `bg-slate-100` / `dark:bg-slate-950`                    |
//! | `--mc-surface`      | `bg-white` / `dark:bg-slate-900`                        |
//! | `--mc-ink`          | `text-slate-950` / `dark:text-slate-100`                |
//! | `--mc-muted`        | `text-slate-500` / `dark:text-slate-400`                |
//! | `--mc-rule`         | `border-slate-200` / `dark:border-slate-700`            |
//! | `--mc-accent-deep`  | `bg-teal-700` / `dark:bg-teal-600`                      |
//! | `--mc-accent`       | `text-teal-600` / `dark:text-teal-400`                  |
//! | `--mc-accent-soft`  | `bg-teal-100` / `dark:bg-teal-900`                      |
//! | `--mc-signal`       | `text-amber-400`                                        |

/// Page shell: mist ground + ink (Release floor `--mc-page` / `--mc-ink`).
pub const PAGE: &str = "flex-1 bg-slate-100 text-slate-950 dark:bg-slate-950 dark:text-slate-100";

/// Raised panel surface + rule (`--mc-surface` / `--mc-rule`).
pub const SURFACE: &str =
	"rounded-2xl border border-slate-200 bg-white p-4 dark:border-slate-700 dark:bg-slate-900";

/// Heading ink.
pub const INK: &str = "text-slate-950 dark:text-slate-100";

/// Secondary copy (`--mc-muted`).
pub const MUTED: &str = "text-sm text-slate-500 dark:text-slate-400";

/// Primary control (`--mc-accent-deep`, 12px radius, ≥44px touch).
pub const BUTTON_PRIMARY: &str =
	"min-h-12 rounded-xl bg-teal-700 px-4 py-3 font-bold text-white dark:bg-teal-600";

/// Outline control on teal brand.
pub const BUTTON_SECONDARY: &str = "min-h-12 rounded-xl border border-teal-300 bg-transparent px-4 py-3 font-bold text-teal-700 dark:text-teal-400";

/// Shipping / live signal (`--mc-signal`).
pub const SIGNAL: &str = "text-amber-400";

/// Section / action gap (`--mc-gap-action` → `1rem`).
pub const GAP_ACTION: &str = "gap-4";
