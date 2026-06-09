//! Public facade for monochange CLI functionality.
//!
//! The implementation lives in `monochange_cli`; this crate keeps the published
//! package and binary name stable while re-exporting the CLI API.

pub use monochange_cli::*;
