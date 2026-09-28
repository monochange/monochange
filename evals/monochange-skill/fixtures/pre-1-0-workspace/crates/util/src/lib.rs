//! Quantity parsing and formatting helpers for the Acme inventory stack.
//!
//! All quantities are stored as whole base units; the helpers here only
//! translate between the stored form and human-readable strings.

/// The unit a quantity is measured in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
	/// Individual items.
	Count,
	/// Millilitres, stored without a fractional part.
	Millilitre,
	/// Grams, stored without a fractional part.
	Gram,
}

impl Unit {
	/// Return the human-readable suffix for the unit.
	#[must_use]
	pub fn suffix(self) -> &'static str {
		match self {
			Self::Count => "pc",
			Self::Millilitre => "ml",
			Self::Gram => "g",
		}
	}
}

/// Parse a human-readable quantity such as `"12 pc"` into base units.
///
/// Returns `None` when the text has no number, the suffix is unknown, or the
/// quantity is negative.
#[must_use]
pub fn parse_quantity(text: &str) -> Option<(Unit, u64)> {
	let text = text.trim();
	let (digits, suffix) = text.split_once(' ')?;
	let amount: u64 = digits.parse().ok()?;
	let unit = match suffix.trim() {
		"pc" => Unit::Count,
		"ml" => Unit::Millilitre,
		"g" => Unit::Gram,
		_ => return None,
	};
	Some((unit, amount))
}

/// Format a quantity as a human-readable string.
#[must_use]
pub fn format_quantity(unit: Unit, amount: u64) -> String {
	format!("{amount} {}", unit.suffix())
}

/// Validate a batch of quantities, returning every problem found.
pub fn validate_batch(entries: &[(Unit, u64)]) -> Vec<String> {
	let mut problems = Vec::new();
	for (unit, amount) in entries {
		if *amount == 0 {
			problems.push(format!("zero quantity is not storable ({})", unit.suffix()));
		}
	}
	problems
}
