//! Version schemes and release value stamping.
//!
//! Template variables come from three sources, and the source decides which
//! ordering guarantees hold:
//!
//! - **context** values (`year`, `month`, `quarter`, `day`, `date`, `time`,
//!   `identity`, and the ordinal counters) are computed, and are monotonic
//!   within their granularity;
//! - **declared values** are configured per package and may be stamped
//!   (incrementing monotonic within their reset policy) or derived
//!   (identifiers with no ordering guarantee at all);
//! - everything else is an **identifier** — a hash, an environment value, or a
//!   git-derived string — which is unique-ish but never ordered.
//!
//! This module owns the pure computation. Callers supply the release timestamp
//! so the engine stays deterministic and testable.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::MonochangeError;
use crate::MonochangeResult;

/// Context variables that declared value ids must not shadow.
///
/// These are reserved because a declared value would otherwise silently change
/// the meaning of an existing template. `build` is deliberately absent: it is
/// the conventional id for a build counter, and `{{ build }}` only resolves
/// when a package declares it.
pub const RESERVED_TEMPLATE_VARIABLES: &[&str] = &[
	"major",
	"minor",
	"patch",
	"version",
	"name",
	"ecosystem",
	"identity",
	"prerelease",
	"year",
	"year_short",
	"month",
	"month_padded",
	"quarter",
	"day",
	"date",
	"time",
	"release_of_month",
	"release_of_quarter",
	"release_of_year",
	"label",
];

/// Hash algorithms available to `hash` value sources.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HashAlgorithm {
	/// SHA-256, the default and only supported algorithm.
	#[default]
	Sha256,
}

impl HashAlgorithm {
	/// Return the canonical config string.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Sha256 => "sha256",
		}
	}
}

/// How a hash is rendered into a template value.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HashEncoding {
	/// Lowercase hexadecimal.
	#[default]
	Hex,
	/// Lowercase RFC 4648 base32 without padding.
	Base32,
	/// Lowercase base36 (`0-9a-z`), the alphanumeric option.
	Base36,
	/// Decimal digits only, derived from the hash as a big-endian integer.
	Digits,
}

impl HashEncoding {
	/// Return the canonical config string.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Hex => "hex",
			Self::Base32 => "base32",
			Self::Base36 => "base36",
			Self::Digits => "digits",
		}
	}
}

/// What happens to a file counter when a release is stamped.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StampBehaviour {
	/// Add one on every stamped release.
	#[default]
	Increment,
	/// Add a fixed amount on every stamped release.
	Add {
		/// Amount added to the current value.
		amount: u64,
	},
	/// Read the value without changing it.
	None,
}

impl StampBehaviour {
	/// Return the canonical config string.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Increment => "increment",
			Self::Add { .. } => "add",
			Self::None => "none",
		}
	}

	/// Apply this behaviour to a current value.
	///
	/// The first stamp of a freshly created counter (`0`) yields `1` for
	/// `increment` and `amount` for `add`.
	#[must_use]
	pub fn apply(self, current: u64) -> u64 {
		match self {
			Self::Increment => current.saturating_add(1),
			Self::Add { amount } => current.saturating_add(amount),
			Self::None => current,
		}
	}

	/// Whether this behaviour mutates the underlying file.
	#[must_use]
	pub fn is_stamped(self) -> bool {
		!matches!(self, Self::None)
	}
}

/// When a stamped counter returns to its starting value.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ResetPolicy {
	/// Never reset: the value only ever increases.
	///
	/// Matches Google Play `versionCode` and macOS `CFBundleVersion`.
	#[default]
	Never,
	/// Reset when the identity version changes, matching Apple's iOS release
	/// trains.
	Version,
}

impl ResetPolicy {
	/// Return the canonical config string.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Never => "never",
			Self::Version => "version",
		}
	}
}

/// Which timestamp a `timestamp` value source reads.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TimestampSource {
	/// The moment the release is prepared, in UTC.
	///
	/// This changes on every run by design; the stamped value is frozen into
	/// the release record so re-rendering is stable.
	#[default]
	Now,
	/// The commit time of the release commit, in UTC.
	Commit,
}

impl TimestampSource {
	/// Return the canonical config string.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Now => "now",
			Self::Commit => "commit",
		}
	}
}

/// Which git-derived value a `git` source reads.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GitSource {
	/// Abbreviated commit hash of the release commit.
	#[default]
	ShortHash,
	/// Number of commits reachable from the release commit.
	CommitCount,
}

impl GitSource {
	/// Return the canonical config string.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::ShortHash => "short_hash",
			Self::CommitCount => "commit_count",
		}
	}
}

/// How a declared value produces its template string.
///
/// Exactly one of the source fields must be set. The sources are declared as
/// sibling optional fields rather than a nested enum so unknown keys fail
/// loudly and validation can report every conflict at once.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueDefinition {
	/// Counter file path, for a stamped or read-only file counter.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub file: Option<PathBuf>,
	/// Dot-separated field path inside `file`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub field: Option<String>,
	/// File to hash, for a derived hash value.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub hash: Option<PathBuf>,
	/// Environment variable name, for an environment-derived value.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub env: Option<String>,
	/// Which git value to read.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub git: Option<GitSource>,
	/// Which timestamp to read.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub timestamp: Option<TimestampSource>,
	/// Hash algorithm. Only meaningful with `hash`.
	#[serde(default)]
	pub algorithm: HashAlgorithm,
	/// How a hash digest is rendered. Only meaningful with `hash`.
	#[serde(default)]
	pub encoding: HashEncoding,
	/// Truncate a rendered hash to this many characters.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub length: Option<usize>,
	/// What happens to a file counter when a release is stamped.
	///
	/// Defaults to `increment` for `file` values and to no stamping for derived
	/// values. Declaring it on a derived value is a config error.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub on_release: Option<StampBehaviour>,
	/// When a stamped counter resets. Only meaningful with `file`.
	#[serde(default)]
	pub reset: ResetPolicy,
}

impl ValueDefinition {
	/// The effective stamp behaviour.
	///
	/// File counters default to `increment`; every derived source defaults to
	/// no stamping.
	#[must_use]
	pub fn stamp_behaviour(&self) -> StampBehaviour {
		match self.on_release {
			Some(behaviour) => behaviour,
			None if self.is_file_counter() => StampBehaviour::Increment,
			None => StampBehaviour::None,
		}
	}

	/// The declared source field names, in the order validation reports them.
	///
	/// Exactly one entry is expected for a valid definition.
	#[must_use]
	pub fn declared_sources(&self) -> Vec<&'static str> {
		let mut sources = Vec::new();
		if self.file.is_some() {
			sources.push("file");
		}
		if self.hash.is_some() {
			sources.push("hash");
		}
		if self.env.is_some() {
			sources.push("env");
		}
		if self.git.is_some() {
			sources.push("git");
		}
		if self.timestamp.is_some() {
			sources.push("timestamp");
		}
		sources
	}

	/// Whether this source is a stamped counter backed by a file.
	#[must_use]
	pub fn is_file_counter(&self) -> bool {
		self.file.is_some()
	}

	/// Whether this value carries an ordering guarantee.
	#[must_use]
	pub fn is_monotonic(&self) -> bool {
		self.is_file_counter() && self.stamp_behaviour().is_stamped()
	}

	/// Whether this value always renders as digits.
	///
	/// File counters hold integers, and `digits`-encoded hashes are decimal.
	/// Everything else may contain letters, which matters when a value lands in
	/// a `SemVer` position.
	#[must_use]
	pub fn renders_numeric(&self) -> bool {
		self.is_file_counter() || (self.hash.is_some() && self.encoding == HashEncoding::Digits)
	}
}

/// One reusable display-label scheme.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionSchemeDefinition {
	/// Template rendered into the package's label.
	pub template: String,
}

/// The context values available to schemes, titles, tags, paths, and values.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LabelInputs {
	/// Release date in `YYYY-MM-DD` form, in UTC.
	#[serde(default)]
	pub date: String,
	/// Release time in `HHMMSS` form, in UTC.
	#[serde(default)]
	pub time: String,
	/// Ordinal of this release within its calendar month.
	#[serde(default)]
	pub of_month: u64,
	/// Ordinal of this release within its calendar quarter.
	#[serde(default)]
	pub of_quarter: u64,
	/// Ordinal of this release within its calendar year.
	#[serde(default)]
	pub of_year: u64,
}

impl LabelInputs {
	/// Whether every field is at its default value.
	///
	/// Used to omit the field from serialized release records written before
	/// label inputs existed.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		self == &Self::default()
	}
}

/// A UTC release timestamp decomposed into its calendar parts.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct ReleaseTimestamp {
	/// Full year, e.g. `2026`.
	pub year: u16,
	/// Month, `1`-`12`.
	pub month: u8,
	/// Day of month, `1`-`31`.
	pub day: u8,
	/// Hour, `0`-`23`.
	pub hour: u8,
	/// Minute, `0`-`59`.
	pub minute: u8,
	/// Second, `0`-`59`.
	pub second: u8,
}

impl ReleaseTimestamp {
	/// Build a timestamp, rejecting out-of-range components.
	pub fn new(
		year: u16,
		month: u8,
		day: u8,
		hour: u8,
		minute: u8,
		second: u8,
	) -> MonochangeResult<Self> {
		if !(1..=12).contains(&month) {
			return Err(MonochangeError::Config(format!(
				"release timestamp month `{month}` is out of range; expected 1-12"
			)));
		}
		if !(1..=31).contains(&day) {
			return Err(MonochangeError::Config(format!(
				"release timestamp day `{day}` is out of range; expected 1-31"
			)));
		}
		if hour > 23 || minute > 59 || second > 59 {
			return Err(MonochangeError::Config(format!(
				"release timestamp `{hour:02}{minute:02}{second:02}` is out of range"
			)));
		}
		Ok(Self {
			year,
			month,
			day,
			hour,
			minute,
			second,
		})
	}

	/// Parse a `YYYY-MM-DD` date pair, rejecting malformed input.
	pub fn parse(date: &str, time: &str) -> MonochangeResult<Self> {
		let mut date_parts = date.split('-');
		let year = date_parts.next().and_then(|part| part.parse::<u16>().ok());
		let month = date_parts.next().and_then(|part| part.parse::<u8>().ok());
		let day = date_parts.next().and_then(|part| part.parse::<u8>().ok());
		if date_parts.next().is_some() {
			return Err(MonochangeError::Config(format!(
				"release timestamp date `{date}` is malformed; expected `YYYY-MM-DD`"
			)));
		}
		let (Some(year), Some(month), Some(day)) = (year, month, day) else {
			return Err(MonochangeError::Config(format!(
				"release timestamp date `{date}` is malformed; expected `YYYY-MM-DD`"
			)));
		};

		let (hour, minute, second) = if time.len() == 6 {
			let hour = time.get(0..2).and_then(|part| part.parse::<u8>().ok());
			let minute = time.get(2..4).and_then(|part| part.parse::<u8>().ok());
			let second = time.get(4..6).and_then(|part| part.parse::<u8>().ok());
			match (hour, minute, second) {
				(Some(hour), Some(minute), Some(second)) => (hour, minute, second),
				_ => {
					return Err(MonochangeError::Config(format!(
						"release timestamp time `{time}` is malformed; expected `HHMMSS`"
					)));
				}
			}
		} else {
			return Err(MonochangeError::Config(format!(
				"release timestamp time `{time}` is malformed; expected `HHMMSS`"
			)));
		};

		Self::new(year, month, day, hour, minute, second)
	}

	/// Two-digit year, e.g. `26`.
	#[must_use]
	pub fn year_short(self) -> u16 {
		self.year % 100
	}

	/// Zero-padded month, e.g. `09`.
	#[must_use]
	pub fn month_padded(self) -> String {
		format!("{:02}", self.month)
	}

	/// Calendar quarter, `1`-`4`.
	#[must_use]
	pub fn quarter(self) -> u8 {
		self.month.saturating_sub(1) / 3 + 1
	}

	/// `YYYY-MM-DD` form.
	#[must_use]
	pub fn date(self) -> String {
		format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
	}

	/// `YYYYMMDD` form.
	#[must_use]
	pub fn date_compact(self) -> String {
		format!("{:04}{:02}{:02}", self.year, self.month, self.day)
	}

	/// `HHMMSS` form.
	#[must_use]
	pub fn time(self) -> String {
		format!("{:02}{:02}{:02}", self.hour, self.minute, self.second)
	}

	/// Whether this timestamp shares the same year as `other`.
	#[must_use]
	pub fn same_year(self, other: &LabelInputs) -> bool {
		other.date.starts_with(&format!("{:04}-", self.year))
	}

	/// Whether this timestamp shares the same month as `other`.
	#[must_use]
	pub fn same_month(self, other: &LabelInputs) -> bool {
		other
			.date
			.starts_with(&format!("{:04}-{:02}-", self.year, self.month))
	}

	/// Whether this timestamp shares the same quarter as `other`.
	#[must_use]
	pub fn same_quarter(self, other: &LabelInputs) -> bool {
		let prefix = format!("{:04}-", self.year);
		let Some(rest) = other.date.strip_prefix(&prefix) else {
			return false;
		};
		let Some(previous_month) = rest.get(0..2).and_then(|part| part.parse::<u8>().ok()) else {
			return false;
		};
		let previous_quarter = previous_month.saturating_sub(1) / 3 + 1;
		previous_quarter == self.quarter()
	}
}

/// Chain ordinal counters from the previous release of the same owner.
///
/// A release in a new month, quarter, or year restarts that counter at `1`.
#[must_use]
pub fn chain_label_inputs(previous: Option<&LabelInputs>, now: ReleaseTimestamp) -> LabelInputs {
	let (of_month, of_quarter, of_year) = match previous {
		Some(previous) => {
			(
				if now.same_month(previous) {
					previous.of_month.saturating_add(1)
				} else {
					1
				},
				if now.same_quarter(previous) {
					previous.of_quarter.saturating_add(1)
				} else {
					1
				},
				if now.same_year(previous) {
					previous.of_year.saturating_add(1)
				} else {
					1
				},
			)
		}
		None => (1, 1, 1),
	};
	LabelInputs {
		date: now.date(),
		time: now.time(),
		of_month,
		of_quarter,
		of_year,
	}
}

/// Render a hash digest into a template value.
#[must_use]
pub fn encode_hash(digest: &[u8], encoding: HashEncoding, length: Option<usize>) -> String {
	let rendered = match encoding {
		HashEncoding::Hex => hex_encode(digest),
		HashEncoding::Base32 => base32_encode(digest),
		HashEncoding::Base36 => base36_encode(digest),
		HashEncoding::Digits => decimal_digits(digest),
	};
	truncate_rendered(rendered, length)
}

fn truncate_rendered(rendered: String, length: Option<usize>) -> String {
	match length {
		Some(0) | None => rendered,
		Some(length) => {
			if encoding_is_right_aligned(length, &rendered) {
				let start = rendered.len().saturating_sub(length);
				rendered.get(start..).unwrap_or(&rendered).to_string()
			} else {
				rendered.chars().take(length).collect()
			}
		}
	}
}

/// Decimal encodings keep the least significant digits; the rest keep the most
/// significant characters.
fn encoding_is_right_aligned(length: usize, rendered: &str) -> bool {
	rendered.len() > length && rendered.chars().all(|character| character.is_ascii_digit())
}

fn hex_encode(digest: &[u8]) -> String {
	use std::fmt::Write;
	let mut rendered = String::with_capacity(digest.len().saturating_mul(2));
	for byte in digest {
		let _ = write!(rendered, "{byte:02x}");
	}
	rendered
}

const BASE32_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
const BASE36_ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

fn base32_encode(digest: &[u8]) -> String {
	encode_bits(digest, 5, BASE32_ALPHABET)
}

fn base36_encode(digest: &[u8]) -> String {
	encode_bits(digest, 6, BASE36_ALPHABET)
}

/// Encode `digest` as base-`bit_width` digits, most significant first, without
/// emitting high-order zero digits.
fn encode_bits(digest: &[u8], bit_width: u32, alphabet: &[u8]) -> String {
	let mut accumulator = 0u32;
	let mut bits = 0u32;
	let mut rendered = String::new();
	for byte in digest {
		accumulator = (accumulator << 8) | u32::from(*byte);
		bits += 8;
		while bits >= bit_width {
			let shift = bits - bit_width;
			let index = (accumulator >> shift) & ((1 << bit_width) - 1);
			let character = alphabet.get(index as usize).copied().unwrap_or(b'0') as char;
			rendered.push(character);
			bits -= bit_width;
			accumulator &= (1 << bits) - 1;
		}
	}
	if bits > 0 {
		let index = (accumulator << (bit_width - bits)) & ((1 << bit_width) - 1);
		let character = alphabet.get(index as usize).copied().unwrap_or(b'0') as char;
		rendered.push(character);
	}
	let trimmed = rendered.trim_start_matches('0');
	if trimmed.is_empty() {
		"0".to_string()
	} else {
		trimmed.to_string()
	}
}

/// Convert a big-endian byte string into its decimal digits.
///
/// Long division is used rather than a bignum dependency: the byte vector is
/// divided by ten repeatedly, collecting remainders least-significant first.
fn decimal_digits(digest: &[u8]) -> String {
	if digest.is_empty() {
		return "0".to_string();
	}
	let mut working = digest.to_vec();
	let mut digits = Vec::new();
	while working.iter().any(|byte| *byte != 0) {
		let mut remainder = 0u32;
		let mut quotient = Vec::with_capacity(working.len());
		for byte in &working {
			let current = remainder * 256 + u32::from(*byte);
			let next = current / 10;
			remainder = current % 10;
			quotient.push(u8::try_from(next).unwrap_or(0));
		}
		digits.push(char::from_digit(remainder, 10).unwrap_or('0').to_string());
		while quotient.first() == Some(&0) {
			quotient.remove(0);
		}
		working = quotient;
	}
	if digits.is_empty() {
		return "0".to_string();
	}
	digits.reverse();
	digits.concat()
}

/// A declared value resolved for one release.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedValue {
	/// The value rendered into templates.
	pub value: String,
	/// Whether this value carries an ordering guarantee.
	pub monotonic: bool,
	/// File-backed counters that must be written back after stamping.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub write_back: Option<CounterWriteBack>,
}

/// A counter file field that must be rewritten with a stamped value.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CounterWriteBack {
	/// Repository-relative path of the counter file.
	pub file: PathBuf,
	/// Dot-separated field path inside that file.
	pub field: String,
	/// Value to write.
	pub value: u64,
}

/// Whether a value definition participates in ordering checks.
#[must_use]
pub fn value_is_monotonic(definition: &ValueDefinition) -> bool {
	definition.is_monotonic()
}

/// Compute a template variable map from context and resolved values.
#[must_use]
pub fn template_variables(
	identity: &str,
	prerelease: &str,
	package_name: &str,
	ecosystem: &str,
	inputs: &LabelInputs,
	values: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
	let mut variables = BTreeMap::new();
	variables.insert("identity".to_string(), identity.to_string());
	variables.insert("prerelease".to_string(), prerelease.to_string());
	variables.insert("version".to_string(), identity.to_string());
	variables.insert("name".to_string(), package_name.to_string());
	variables.insert("ecosystem".to_string(), ecosystem.to_string());
	variables.insert("date".to_string(), inputs.date.replace('-', ""));
	variables.insert("time".to_string(), inputs.time.clone());
	variables.insert("release_of_month".to_string(), inputs.of_month.to_string());
	variables.insert(
		"release_of_quarter".to_string(),
		inputs.of_quarter.to_string(),
	);
	variables.insert("release_of_year".to_string(), inputs.of_year.to_string());
	if let Ok(timestamp) = ReleaseTimestamp::parse(&inputs.date, &inputs.time) {
		variables.insert("year".to_string(), timestamp.year.to_string());
		variables.insert("year_short".to_string(), timestamp.year_short().to_string());
		variables.insert("month".to_string(), timestamp.month.to_string());
		variables.insert("month_padded".to_string(), timestamp.month_padded());
		variables.insert("quarter".to_string(), timestamp.quarter().to_string());
		variables.insert("day".to_string(), timestamp.day.to_string());
	}
	for (key, value) in values {
		variables.insert(key.clone(), value.clone());
	}
	variables
}

/// Errors produced while reading a declared counter file.
#[derive(Debug)]
#[non_exhaustive]
pub enum CounterFileError {
	/// The declared file does not exist.
	MissingFile {
		/// Repository-relative path of the missing file.
		path: PathBuf,
	},
	/// The file parsed, but the declared field is absent.
	MissingField {
		/// Repository-relative path of the counter file.
		path: PathBuf,
		/// The declared dot-separated field path.
		field: String,
	},
	/// The declared field holds something other than an integer.
	NotAnInteger {
		/// Repository-relative path of the counter file.
		path: PathBuf,
		/// The declared dot-separated field path.
		field: String,
		/// The value that was found.
		found: String,
	},
}

impl fmt::Display for CounterFileError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::MissingFile { path } => {
				write!(
					formatter,
					"counter file `{}` does not exist; create it with its starting value, for example {{\"build\": 0}}",
					path.display()
				)
			}
			Self::MissingField { path, field } => {
				write!(
					formatter,
					"counter file `{}` does not contain field `{field}`; add the field with an integer starting value",
					path.display()
				)
			}
			Self::NotAnInteger { path, field, found } => {
				write!(
					formatter,
					"counter file `{}` field `{field}` holds `{found}`; counters must be non-negative integers",
					path.display()
				)
			}
		}
	}
}

impl std::error::Error for CounterFileError {}

impl From<CounterFileError> for MonochangeError {
	fn from(error: CounterFileError) -> Self {
		Self::Config(error.to_string())
	}
}

/// Read an integer counter from a parsed document using a dotted field path.
pub fn counter_from_json(
	document: &serde_json::Value,
	path: &Path,
	field: &str,
) -> Result<u64, CounterFileError> {
	let mut cursor = document;
	for segment in field.split('.') {
		let Some(next) = cursor.get(segment) else {
			return Err(CounterFileError::MissingField {
				path: path.to_path_buf(),
				field: field.to_string(),
			});
		};
		cursor = next;
	}
	cursor.as_u64().ok_or_else(|| {
		CounterFileError::NotAnInteger {
			path: path.to_path_buf(),
			field: field.to_string(),
			found: render_json_scalar(cursor),
		}
	})
}

fn render_json_scalar(value: &serde_json::Value) -> String {
	match value {
		serde_json::Value::String(text) => text.clone(),
		other => other.to_string(),
	}
}

/// Return the dotted field path segments for a declared counter field.
pub fn counter_field_segments(field: &str) -> Vec<&str> {
	field
		.split('.')
		.filter(|segment| !segment.is_empty())
		.collect()
}

/// Validate one declared value definition.
///
/// Rejects definitions that declare no source, several sources, a field-less or
/// path-less file counter, and hash-only options used without `hash`.
pub fn validate_value_definition(id: &str, definition: &ValueDefinition) -> MonochangeResult<()> {
	// Check the file/field pairing first: `field` alone is a common typo, and
	// the paired error is more useful than a generic "no source" message.
	if definition.file.is_some() && definition.field.is_none() {
		return Err(MonochangeError::Config(format!(
			"value `{id}` sets `file` without `field`; declare the dot-separated field that holds the counter"
		)));
	}
	if definition.field.is_some() && definition.file.is_none() {
		return Err(MonochangeError::Config(format!(
			"value `{id}` sets `field` without `file`; declare the file that holds the counter"
		)));
	}
	if let Some(field) = definition.field.as_deref()
		&& counter_field_segments(field).is_empty()
	{
		return Err(MonochangeError::Config(format!(
			"value `{id}` has an empty `field`; declare at least one dot-separated segment"
		)));
	}

	let sources = definition.declared_sources();
	if sources.is_empty() {
		return Err(MonochangeError::Config(format!(
			"value `{id}` declares no source; set exactly one of `file`, `hash`, `env`, `git`, or `timestamp`"
		)));
	}
	if sources.len() > 1 {
		return Err(MonochangeError::Config(format!(
			"value `{id}` declares multiple sources ({}); set exactly one of `file`, `hash`, `env`, `git`, or `timestamp`",
			sources.join(", ")
		)));
	}

	if definition.hash.is_none() {
		if definition.length.is_some() {
			return Err(MonochangeError::Config(format!(
				"value `{id}` sets `length` without `hash`; `length` only applies to hash values"
			)));
		}
		if definition.algorithm != HashAlgorithm::default()
			|| definition.encoding != HashEncoding::default()
		{
			return Err(MonochangeError::Config(format!(
				"value `{id}` sets hash options without `hash`; `algorithm` and `encoding` only apply to hash values"
			)));
		}
	}
	if !definition.is_file_counter() {
		if definition.reset != ResetPolicy::default() {
			return Err(MonochangeError::Config(format!(
				"value `{id}` sets `reset` without a file counter; `reset` only applies to `file` values"
			)));
		}
		if definition.on_release.is_some() {
			return Err(MonochangeError::Config(format!(
				"value `{id}` sets `on_release` without a file counter; derived values are read-only"
			)));
		}
	}
	Ok(())
}

/// Validate a declared value id.
pub fn validate_value_id(id: &str) -> MonochangeResult<()> {
	if id.is_empty() {
		return Err(MonochangeError::Config(
			"value ids must not be empty".to_string(),
		));
	}
	if RESERVED_TEMPLATE_VARIABLES.contains(&id) {
		return Err(MonochangeError::Config(format!(
			"value id `{id}` is a reserved template variable; rename the value (reserved: {})",
			RESERVED_TEMPLATE_VARIABLES.join(", ")
		)));
	}
	let valid = id.chars().all(|character| {
		character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
	});
	if !valid {
		return Err(MonochangeError::Config(format!(
			"value id `{id}` must use lowercase letters, digits, and underscores"
		)));
	}
	if id.starts_with(|character: char| character.is_ascii_digit()) {
		return Err(MonochangeError::Config(format!(
			"value id `{id}` must not start with a digit"
		)));
	}
	Ok(())
}

/// Extract every template variable name referenced by `template`.
#[must_use]
pub fn template_variables_used(template: &str) -> Vec<String> {
	let mut variables = Vec::new();
	let mut rest = template;
	while let Some(start) = rest.find("{{") {
		let after_start = &rest[start + 2..];
		let Some(end) = after_start.find("}}") else {
			break;
		};
		let variable = after_start[..end].trim().to_string();
		if !variable.is_empty() {
			variables.push(variable);
		}
		rest = &after_start[end + 2..];
	}
	variables
}

/// Validate that every variable in `template` is available.
///
/// `available` holds the context variable names plus the package's declared
/// value ids. `surface` names the config key for the error message.
pub fn validate_template_variables(
	template: &str,
	available: &BTreeMap<String, String>,
	surface: &str,
) -> MonochangeResult<()> {
	for variable in template_variables_used(template) {
		if variable.starts_with("build.") {
			let axis = variable.trim_start_matches("build.");
			if !available.contains_key(axis) {
				return Err(MonochangeError::Config(format!(
					"{surface} uses unknown value `{axis}`; declared values are: {}",
					render_available_values(available)
				)));
			}
			continue;
		}
		if !available.contains_key(&variable) {
			return Err(MonochangeError::Config(format!(
				"{surface} uses unknown variable `{{{{ {variable} }}}}`; available variables are: {}",
				render_available_variables(available)
			)));
		}
	}
	Ok(())
}

fn render_available_values(available: &BTreeMap<String, String>) -> String {
	let values = available
		.keys()
		.filter(|key| !RESERVED_TEMPLATE_VARIABLES.contains(&key.as_str()))
		.cloned()
		.collect::<Vec<_>>();
	if values.is_empty() {
		return "none declared".to_string();
	}
	values.join(", ")
}

fn render_available_variables(available: &BTreeMap<String, String>) -> String {
	available.keys().cloned().collect::<Vec<_>>().join(", ")
}

/// Render a version template with the supplied variables.
///
/// Substitution is longest-name-first so `build.android` cannot be mangled by a
/// shorter `build` entry.
#[must_use]
pub fn render_version_template(template: &str, variables: &BTreeMap<String, String>) -> String {
	let mut rendered = template.to_string();
	let mut names = variables.keys().collect::<Vec<_>>();
	names.sort_by_key(|name| std::cmp::Reverse(name.len()));
	for name in names {
		let Some(value) = variables.get(name) else {
			continue;
		};
		for pattern in [format!("{{{{ {name} }}}}"), format!("{{{{{name}}}}}")] {
			rendered = rendered.replace(&pattern, value);
		}
	}
	rendered
}

#[cfg(test)]
#[path = "__tests__/versioning_tests.rs"]
mod tests;
