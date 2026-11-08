//! Session token kept in extension storage across popup, options, and worker.

/// Storage key shared by every extension surface.
pub const SESSION_KEY: &str = "monochange.session";

/// Parses a kept session string into a bearer token, accepting either a raw
/// token or the JSON `{ "access_token": "…" }` shape native hosts use.
#[must_use]
pub fn bearer_from_kept(kept: &str) -> Option<String> {
	let trimmed = kept.trim();
	if trimmed.is_empty() {
		return None;
	}
	if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
		return value
			.get("access_token")
			.and_then(|token| token.as_str())
			.filter(|token| !token.is_empty())
			.map(str::to_owned);
	}
	Some(trimmed.to_owned())
}
