//! Feedback server functions.

use leptos::server;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackSubmission {
	pub email: Option<String>,
	pub feedback: String,
	pub category: Option<String>,
}

// Leptos server functions must be async.
#[allow(clippy::unused_async)]
#[server]
pub async fn submit_feedback(
	repo_slug: String,
	form_slug: String,
	submission: FeedbackSubmission,
) -> Result<(), server_fn::ServerFnError> {
	let _ = (repo_slug, form_slug, submission);
	Ok(())
}
