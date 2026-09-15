//! Change classification for monochange.
//!
//! Owns the [`classification report`](ChangeClassificationReport) contract
//! written by `monochange change classify`, the compatibility findings that
//! feed it, and the CLI command-surface snapshots used as a finding source.
//!
//! The report contract is versioned independently of the monochange release
//! train through this crate's `SCHEMA_VERSION`, mirroring the durable schema
//! pattern used for configuration and release records.

#![allow(unstable_features)]
#![cfg_attr(test, allow(unused_imports, unused_qualifications))]
#![feature(coverage_attribute)]

/// Raw classification schema version text embedded at compile time from the
/// `SCHEMA_VERSION` file.
pub const SCHEMA_VERSION: &str = include_str!("../SCHEMA_VERSION").trim_ascii_end();

pub mod cli_surface;
#[cfg(feature = "schema")]
pub mod schema;

mod classification;

pub use classification::*;
