//! Hard limits for hosted Markdown bodies.

/// Keep the longest prefix ending at a supplied byte boundary that fits, including
/// `notice`. Boundaries should delimit complete packages or release-note entries.
/// Bodies that already fit are returned unchanged. If even the notice cannot fit,
/// return its character-safe prefix; the configured cap is always respected.
#[must_use]
pub fn bound_body(body: &str, boundaries: &[usize], notice: &str, max_chars: usize) -> String {
	if body.chars().count() <= max_chars {
		return body.to_owned();
	}
	let notice_chars = notice.chars().count();
	if notice_chars > max_chars {
		return notice.chars().take(max_chars).collect();
	}
	let budget = max_chars - notice_chars;
	let end = boundaries
		.iter()
		.copied()
		.filter(|&end| {
			body.get(..end)
				.is_some_and(|prefix| prefix.chars().count() <= budget)
		})
		.max()
		.unwrap_or(0);
	format!("{}{notice}", body.get(..end).unwrap_or_default())
}

#[cfg(test)]
#[path = "__tests__/body_limit_tests.rs"]
mod tests;
