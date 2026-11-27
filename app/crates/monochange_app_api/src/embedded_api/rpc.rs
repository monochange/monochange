//! The embedded feedback protocol as transport-independent calls.
//!
//! The leptoskit RPC operations (`monochange_app::server_fns::feedback_sdk`)
//! call these with the request's headers, which carry the integration,
//! session, or worker credential, the `Origin`, and the `Idempotency-Key`,
//! exactly as the REST routes under `/api/v1/feedback/{project}` receive
//! them. Each call runs the same handler as its REST route.

use axum::Json;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum_extra::extract::cookie::CookieJar;
use serde_json::Value;

use super::ApiFailure;
use super::Parameters;
pub use super::client::DraftInput;
pub use super::client::MessageInput;
pub use super::client::ReplyInput;
pub use super::client::SessionInput;
pub use super::client::SubmitInput;
pub use super::client::SuggestInput;
pub use super::iframe::SessionInput as IframeSessionInput;
pub use super::worker::Heartbeat;
pub use super::worker::WorkerResult;
use crate::AppState;

/// One call's state, project, and request headers.
#[derive(Clone)]
pub struct Call {
	pub state: AppState,
	pub project: i32,
	pub headers: HeaderMap,
}

impl Call {
	fn state(&self) -> State<AppState> {
		State(self.state.clone())
	}

	fn path(&self, id: Option<String>) -> Path<Parameters> {
		Path(Parameters {
			project: self.project,
			id,
		})
	}

	fn headers(&self) -> HeaderMap {
		self.headers.clone()
	}
}

type Answer = Result<Value, ApiFailure>;

fn answer(result: Result<Json<Value>, ApiFailure>) -> Answer {
	result.map(|Json(value)| value)
}

/// Issue a session for a host app's signed-in user.
pub async fn session(call: Call, input: SessionInput) -> Answer {
	answer(
		super::client::session(
			call.state(),
			call.path(None),
			call.headers(),
			Ok(Json(input)),
		)
		.await,
	)
}

/// Suggest similar requests for a draft.
pub async fn suggestions(call: Call, input: SuggestInput) -> Answer {
	super::client::suggestions(
		call.state(),
		call.path(None),
		call.headers(),
		Ok(Json(input)),
	)
	.await
	.map(|Json(reply)| serde_json::to_value(reply).unwrap_or(Value::Null))
}

/// Start a draft conversation with the assistant.
pub async fn create_conversation(call: Call, input: DraftInput) -> Answer {
	answer(
		super::client::create_conversation(
			call.state(),
			call.path(None),
			call.headers(),
			Ok(Json(input)),
		)
		.await,
	)
}

/// Read a draft conversation.
pub async fn conversation(call: Call, id: String) -> Answer {
	answer(
		super::client::show_conversation(call.state(), call.path(Some(id)), call.headers()).await,
	)
}

/// Add the user's next message to a draft conversation.
pub async fn conversation_message(call: Call, id: String, input: MessageInput) -> Answer {
	answer(
		super::client::conversation_message(
			call.state(),
			call.path(Some(id)),
			call.headers(),
			Ok(Json(input)),
		)
		.await,
	)
}

/// Submit a draft conversation as a feedback item.
pub async fn submit(call: Call, input: SubmitInput) -> Answer {
	answer(
		super::client::submit(
			call.state(),
			call.path(None),
			call.headers(),
			Ok(Json(input)),
		)
		.await,
	)
}

/// Read a feedback item the user may see.
pub async fn item(call: Call, id: String) -> Answer {
	answer(super::client::item(call.state(), call.path(Some(id)), call.headers()).await)
}

/// Reply on a feedback item.
pub async fn reply(call: Call, id: String, input: ReplyInput) -> Answer {
	answer(
		super::client::reply(
			call.state(),
			call.path(Some(id)),
			call.headers(),
			Ok(Json(input)),
		)
		.await,
	)
}

/// Vote for a public item, or take the vote back.
pub async fn vote(call: Call, id: String, voted: bool) -> Answer {
	let (state, path, headers) = (call.state(), call.path(Some(id)), call.headers());
	answer(if voted {
		super::client::vote(state, path, headers).await
	} else {
		super::client::unvote(state, path, headers).await
	})
}

/// Pair a worker with its one-time pairing code.
pub async fn worker_pair(call: Call) -> Answer {
	answer(super::worker::pair(call.state(), call.path(None), call.headers()).await)
}

/// Claim the next job for a paired worker.
pub async fn worker_claim(call: Call) -> Answer {
	answer(super::worker::claim(call.state(), call.path(None), call.headers()).await)
}

/// Extend a worker's lease on a job.
pub async fn job_heartbeat(call: Call, id: String, input: Heartbeat) -> Answer {
	answer(
		super::worker::heartbeat(
			call.state(),
			call.path(Some(id)),
			call.headers(),
			Ok(Json(input)),
		)
		.await,
	)
}

/// Return a job's plan, changes, failure, or cancellation.
pub async fn job_result(call: Call, id: String, input: WorkerResult) -> Answer {
	answer(
		super::worker::result(
			call.state(),
			call.path(Some(id)),
			call.headers(),
			Ok(Json(input)),
		)
		.await,
	)
}

/// Exchange the signed-in browser session for a hosted frame session.
pub async fn iframe_session(
	state: &AppState,
	headers: &HeaderMap,
	input: IframeSessionInput,
) -> Answer {
	super::iframe::issue(state, headers, &CookieJar::from_headers(headers), input).await
}
