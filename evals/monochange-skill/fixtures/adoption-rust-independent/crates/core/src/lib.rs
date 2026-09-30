//! Parsing and validation primitives shared by the Acme toolchain.

/// A parsed configuration document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
	/// Raw source text the document was parsed from.
	pub source: String,
}

impl Document {
	/// Parse a document from source text.
	pub fn parse(source: &str) -> Self {
		Self { source: source.to_string() }
	}

	/// Return the number of non-empty lines in the document.
	pub fn line_count(&self) -> usize {
		self.source.lines().filter(|line| !line.trim().is_empty()).count()
	}
}

/// Validate a document, returning every problem found.
pub fn validate(document: &Document) -> Vec<String> {
	if document.source.trim().is_empty() {
		return vec!["document is empty".to_string()];
	}
	Vec::new()
}
