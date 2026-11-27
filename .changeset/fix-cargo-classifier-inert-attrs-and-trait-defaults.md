---
monochange_cargo: fix
---

# Ignore inert Rust attributes and classify defaulted trait additions as additive

`cargo/public-api` now strips inert lint-level attributes (`allow`, `warn`, `deny`, `forbid`, `expect`), `rustfmt::skip`, `track_caller`, and `doc` attributes (including inside `#[cfg_attr(...)]`) consistently across every Rust item kind and nested fields, variants, and trait items. On items whose outer attributes are already part of the tracked signature (`const`, `static`, `struct`, `enum`, `union`, `trait`, `type`, `mod`, and trait members), attribute-only deltas limited to `#[inline]`, `#[must_use]`, and `#[deprecated]` are modeled as compatible changes (`patch`, with only newly deprecated items proposing `minor`). Standalone `pub fn` and `pub use` items keep their existing attribute-free signature collection so proc-macro and tool attributes such as `#[tracing::instrument]` or `#[clippy::msrv]` do not trigger false breaking findings.

Public traits now use a structured surface comparison: adding a dyn-compatible defaulted method, adding a defaulted member with `where Self: Sized`, adding a defaulted member to a trait that was already syntactically non-dyn-compatible, or adding a default to an existing required item classifies as `additive` (`minor`, medium confidence, partial coverage). Defaulted additions that break dyn-compatibility on a previously dyn-compatible trait (generic methods, `async fn`, receiverless functions, methods referencing `Self` outside the receiver or `Self::Assoc` projections, associated constants, or nightly associated-type defaults without `where Self: Sized`), required additions, signature changes, removals, and trait header changes remain `breaking` (`major`).

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
