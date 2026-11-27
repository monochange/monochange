---
monochange_cargo: fix
---

# Ignore inert Rust attributes and classify defaulted trait additions as additive

`cargo/public-api` now strips inert lint-level attributes (`allow`, `warn`, `deny`, `forbid`, `expect`), `rustfmt::skip`, and documentation prose/presentation options (including inside `#[cfg_attr(...)]`) consistently across every Rust item kind while preserving `#[doc(hidden)]` and other API- or ABI-significant attributes (`#[repr(...)]`, `#[non_exhaustive]`, `#[cfg(...)]`, `#[derive(...)]`, `#[macro_export]`, `#[automatically_derived]`, `#[track_caller]`, `#[unsafe(no_mangle)]`, `#[unsafe(export_name = ...)]`, `#[unsafe(link_name = ...)]`, and unknown proc-macro attributes) on functions, re-exports, and `#[macro_export]` macros as well. Attribute-only changes limited to `#[inline]`, `#[must_use]`, and `#[deprecated]` are modeled as compatible changes instead of breaking major bumps.

Public traits that only gain methods, associated constants, or associated types with defaults (or add defaults to existing required items) are now classified as `additive` (`minor`, medium confidence, partial coverage) rather than `breaking` (`major`). Required additions, signature changes, removals, supertrait/generic/`where`-clause changes, `unsafe` transitions, and detectable sealing changes remain `breaking`.

```rust
// Previously reported as breaking/major; now produces no API finding:
#[allow(rustdoc::broken_intra_doc_links, clippy::doc_lazy_continuation)]
pub const PINA_BONDING_CURVE_ID: Address = Address("Curve1Gq...");

// Previously reported as breaking/major; now classified as additive/minor:
pub trait ParseAccounts<'a>: Sized {
	fn parse_accounts(accounts: &'a [AccountInfo]) -> Result<Self, ProgramError>;

	#[doc(hidden)]
	fn __try_from_account_infos(accounts: &'a [AccountInfo]) -> Result<Self, ProgramError> {
		Self::parse_accounts(accounts)
	}
}
```
