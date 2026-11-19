//! The embedded feedback SDK and connected workers: their RPC.
//!
//! `@monochange/feedback`, its React Native and Flutter clients, and
//! `@monochange/feedback-worker` call these leptoskit RPC operations. Each
//! takes the numeric `project` as an argument and authenticates with the
//! same credentials as the REST routes under `/api/v1/feedback/{project}`:
//! the integration credential (`mcfi.`) for sessions, a session token
//! (`mcfs.`) for the widget, a pairing code (`mcfp.`) or worker token
//! (`mcfw.`) for workers, sent as `Authorization: Bearer`.
//!
//! Browsers call cross-origin, so the project must also be in the query,
//! `POST /api/feedback_sdk_submit?project=42`: a CORS preflight carries no
//! body, and [`sdk_gate`] answers it, and every request with an `Origin`,
//! only for origins the project's web integrations register. An operation
//! refuses a body whose `project` differs from the query's. Every answer is
//! `Cache-Control: no-store`, and a `429` carries `Retry-After`.
//!
//! Failures answer with their HTTP status and a [`FeedbackRpcError`] JSON
//! body, with the REST routes' machine-readable `code`. The hosted frame's
//! session exchange, `feedback_iframe_session`, is a first-party browser
//! call authenticated by the website's session cookie.

#[cfg(test)]
#[path = "__tests__/feedback_sdk_tests.rs"]
mod tests;

use std::sync::Arc;

use axum::extract::Request;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use leptos::prelude::expect_context;
use leptos::server;
use leptoskit_rpc::api;
use monochange_app_api::AppState;
use monochange_app_api::embedded_api::ApiFailure;
use monochange_app_api::embedded_api::rpc;
use monochange_app_api::embedded_api::rpc::Call;
use monochange_app_api::embedded_api::rpc::DraftInput;
use monochange_app_api::embedded_api::rpc::Heartbeat;
use monochange_app_api::embedded_api::rpc::IframeSessionInput;
use monochange_app_api::embedded_api::rpc::MessageInput;
use monochange_app_api::embedded_api::rpc::ReplyInput;
use monochange_app_api::embedded_api::rpc::SessionInput;
use monochange_app_api::embedded_api::rpc::SubmitInput;
use monochange_app_api::embedded_api::rpc::SuggestInput;
use monochange_app_api::embedded_api::rpc::WorkerResult;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use server_fn::ServerFn;
use server_fn::codec::Json;
use server_fn::codec::JsonEncoding;
use server_fn::error::FromServerFnError;
use server_fn::error::ServerFnErrorErr;

/// A feedback operation's failure: the HTTP status, the REST routes'
/// machine-readable code, such as `rate_limited`, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackRpcError {
	/// The HTTP status, such as `401` or `429`.
	pub status: u16,
	/// A stable code: `unauthorized`, `not_found`, `conflict`,
	/// `rate_limited`, `invalid_input`, `origin_denied`, `unavailable`, ...
	pub code: String,
	/// What went wrong.
	pub message: String,
}

impl std::fmt::Display for FeedbackRpcError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "HTTP {} {}: {}", self.status, self.code, self.message)
	}
}

impl From<ApiFailure> for FeedbackRpcError {
	fn from(failure: ApiFailure) -> Self {
		Self {
			status: failure.status.as_u16(),
			code: failure.code.to_string(),
			message: failure.message,
		}
	}
}

impl FromServerFnError for FeedbackRpcError {
	type Encoder = JsonEncoding;

	fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
		let status = super::hosted::framework_status(&value);
		let code = if status == StatusCode::INTERNAL_SERVER_ERROR {
			"unavailable"
		} else {
			"invalid_input"
		};
		Self {
			status: status.as_u16(),
			code: code.to_string(),
			message: value.to_string(),
		}
	}

	fn status_code(&self) -> StatusCode {
		StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
	}
}

/// The project a browser request named in its `?project=` query, which
/// [`sdk_gate`] checked the request's `Origin` against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryProject(pub i32);

type Answer = Result<Value, FeedbackRpcError>;

/// Run `operation` for `project` with this request's headers.
async fn with_call<F>(project: i32, operation: impl FnOnce(Call) -> F) -> Answer
where
	F: Future<Output = Result<Value, ApiFailure>>,
{
	let state = expect_context::<Arc<AppState>>();
	let parts = expect_context::<axum::http::request::Parts>();

	if parts
		.extensions
		.get::<QueryProject>()
		.is_some_and(|QueryProject(query)| *query != project)
	{
		return Err(FeedbackRpcError {
			status: StatusCode::BAD_REQUEST.as_u16(),
			code: "invalid_input".to_string(),
			message: "the `project` argument must match the `?project=` query".to_string(),
		});
	}

	operation(Call {
		state: state.as_ref().clone(),
		project,
		headers: parts.headers,
	})
	.await
	.map_err(FeedbackRpcError::from)
}

/// Issue a ten-minute widget session for the host app's signed-in user.
///
/// Called by the host app's backend with its integration credential, never
/// from a browser.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `input` - The host's stable user id and the widget's surface.
///
/// # Errors
///
/// `400` when called from a browser or with an invalid user id, `401` for
/// an invalid integration credential, `429` past the session quotas.
#[api(crate = "leptoskit_rpc", tag = "feedback-sdk")]
#[server(endpoint = "feedback_sdk_session", input = Json, output = Json)]
pub async fn feedback_sdk_session(project: i32, input: SessionInput) -> Answer {
	with_call(project, |call| rpc::session(call, input)).await
}

/// Suggest existing requests similar to a draft.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `input` - The draft text.
///
/// # Errors
///
/// `401` for an invalid session, `429` past the assistant quota.
#[api(crate = "leptoskit_rpc", tag = "feedback-sdk")]
#[server(endpoint = "feedback_sdk_suggestions", input = Json, output = Json)]
pub async fn feedback_sdk_suggestions(project: i32, input: SuggestInput) -> Answer {
	with_call(project, |call| rpc::suggestions(call, input)).await
}

/// Start a draft conversation with the feedback assistant.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `input` - The kind of feedback, the first message, and page context.
///
/// # Errors
///
/// `401` for an invalid session, `429` past the assistant quota.
#[api(crate = "leptoskit_rpc", tag = "feedback-sdk")]
#[server(endpoint = "feedback_sdk_create_conversation", input = Json, output = Json)]
pub async fn feedback_sdk_create_conversation(project: i32, input: DraftInput) -> Answer {
	with_call(project, |call| rpc::create_conversation(call, input)).await
}

/// Read one of the user's draft conversations.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `id` - The conversation id.
///
/// # Errors
///
/// `401` for an invalid session, `404` for another user's conversation.
#[api(crate = "leptoskit_rpc", tag = "feedback-sdk")]
#[server(endpoint = "feedback_sdk_conversation", input = Json, output = Json)]
pub async fn feedback_sdk_conversation(project: i32, id: String) -> Answer {
	with_call(project, |call| rpc::conversation(call, id)).await
}

/// Add the user's next message to a draft conversation.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `id` - The conversation id.
/// * `input` - The message and the conversation version it answers.
///
/// # Errors
///
/// `401` for an invalid session, `409` when the version is stale.
#[api(crate = "leptoskit_rpc", tag = "feedback-sdk")]
#[server(endpoint = "feedback_sdk_conversation_message", input = Json, output = Json)]
pub async fn feedback_sdk_conversation_message(
	project: i32,
	id: String,
	input: MessageInput,
) -> Answer {
	with_call(project, |call| rpc::conversation_message(call, id, input)).await
}

/// Submit a draft conversation as a feedback item. Send an
/// `Idempotency-Key` header: a retried submission returns the first receipt.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `input` - The conversation and the version being submitted.
///
/// # Errors
///
/// `400` without an `Idempotency-Key`, `401` for an invalid session, `409`
/// when the version is stale.
#[api(crate = "leptoskit_rpc", tag = "feedback-sdk")]
#[server(endpoint = "feedback_sdk_submit", input = Json, output = Json)]
pub async fn feedback_sdk_submit(project: i32, input: SubmitInput) -> Answer {
	with_call(project, |call| rpc::submit(call, input)).await
}

/// Read a feedback item: the user's own, or one on the public roadmap.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `id` - The feedback item id.
///
/// # Errors
///
/// `401` for an invalid session, `404` for an item the user may not see.
#[api(crate = "leptoskit_rpc", tag = "feedback-sdk")]
#[server(endpoint = "feedback_sdk_item", input = Json, output = Json)]
pub async fn feedback_sdk_item(project: i32, id: String) -> Answer {
	with_call(project, |call| rpc::item(call, id)).await
}

/// Reply on a feedback item.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `id` - The feedback item id.
/// * `input` - The reply.
///
/// # Errors
///
/// `401` for an invalid session, `404` for an item the user may not see.
#[api(crate = "leptoskit_rpc", tag = "feedback-sdk")]
#[server(endpoint = "feedback_sdk_reply", input = Json, output = Json)]
pub async fn feedback_sdk_reply(project: i32, id: String, input: ReplyInput) -> Answer {
	with_call(project, |call| rpc::reply(call, id, input)).await
}

/// Vote for a public roadmap item, or take the vote back.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `id` - The feedback item id.
/// * `voted` - `true` to vote, `false` to take the vote back.
///
/// # Errors
///
/// `401` for an invalid session, `404` for an item not on the roadmap.
#[api(crate = "leptoskit_rpc", tag = "feedback-sdk")]
#[server(endpoint = "feedback_sdk_vote", input = Json, output = Json)]
pub async fn feedback_sdk_vote(project: i32, id: String, voted: bool) -> Answer {
	with_call(project, |call| rpc::vote(call, id, voted)).await
}

/// Pair a worker using its one-time pairing code as the bearer.
///
/// # Arguments
///
/// * `project` - The monochange project id.
///
/// # Errors
///
/// `401` for an invalid or expired pairing code, or a browser caller.
#[api(crate = "leptoskit_rpc", tag = "feedback-worker")]
#[server(endpoint = "feedback_worker_pair", input = Json, output = Json)]
pub async fn feedback_worker_pair(project: i32) -> Answer {
	with_call(project, rpc::worker_pair).await
}

/// Claim the next planning or implementation job for this worker.
///
/// # Arguments
///
/// * `project` - The monochange project id.
///
/// # Errors
///
/// `401` for an invalid worker token.
#[api(crate = "leptoskit_rpc", tag = "feedback-worker")]
#[server(endpoint = "feedback_worker_claim", input = Json, output = Json)]
pub async fn feedback_worker_claim(project: i32) -> Answer {
	with_call(project, rpc::worker_claim).await
}

/// Extend the worker's lease on a job, and learn whether to cancel it.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `id` - The job id.
/// * `input` - The lease the claim returned.
///
/// # Errors
///
/// `401` for an invalid worker token or lease.
#[api(crate = "leptoskit_rpc", tag = "feedback-worker")]
#[server(endpoint = "feedback_worker_heartbeat", input = Json, output = Json)]
pub async fn feedback_worker_heartbeat(project: i32, id: String, input: Heartbeat) -> Answer {
	with_call(project, |call| rpc::job_heartbeat(call, id, input)).await
}

/// Return a job's plan, file changes, failure, or cancellation.
///
/// # Arguments
///
/// * `project` - The monochange project id.
/// * `id` - The job id.
/// * `input` - The lease and the outcome.
///
/// # Errors
///
/// `401` for an invalid worker token or lease, `409` when the job moved on.
#[api(crate = "leptoskit_rpc", tag = "feedback-worker")]
#[server(endpoint = "feedback_worker_result", input = Json, output = Json)]
pub async fn feedback_worker_result(project: i32, id: String, input: WorkerResult) -> Answer {
	with_call(project, |call| rpc::job_result(call, id, input)).await
}

/// Exchange the signed-in website session for a hosted feedback frame
/// session. Only the website's own pages may call it.
///
/// # Arguments
///
/// * `input` - The organization and project slugs of the portal.
///
/// # Errors
///
/// `401` from another origin or without a website session, `404` for an
/// unknown project.
#[api(crate = "leptoskit_rpc", tag = "feedback-sdk")]
#[server(endpoint = "feedback_iframe_session", input = Json, output = Json)]
pub async fn feedback_iframe_session(input: IframeSessionInput) -> Answer {
	let state = expect_context::<Arc<AppState>>();
	let parts = expect_context::<axum::http::request::Parts>();
	rpc::iframe_session(&state, &parts.headers, input)
		.await
		.map_err(FeedbackRpcError::from)
}

/// The operations [`sdk_gate`] serves cross-origin. The frame session is
/// first-party and stays out.
fn sdk_paths() -> [&'static str; 13] {
	[
		FeedbackSdkSession::PATH,
		FeedbackSdkSuggestions::PATH,
		FeedbackSdkCreateConversation::PATH,
		FeedbackSdkConversation::PATH,
		FeedbackSdkConversationMessage::PATH,
		FeedbackSdkSubmit::PATH,
		FeedbackSdkItem::PATH,
		FeedbackSdkReply::PATH,
		FeedbackSdkVote::PATH,
		FeedbackWorkerPair::PATH,
		FeedbackWorkerClaim::PATH,
		FeedbackWorkerHeartbeat::PATH,
		FeedbackWorkerResult::PATH,
	]
}

/// Whether `path` is an SDK or worker operation, authenticated by its own
/// feedback credential rather than a website session or client token.
#[must_use]
pub fn is_sdk_path(path: &str) -> bool {
	sdk_paths().contains(&path)
}

fn query_project(request: &Request) -> Option<i32> {
	request
		.uri()
		.query()?
		.split('&')
		.find_map(|pair| pair.strip_prefix("project="))
		.and_then(|value| value.parse().ok())
}

/// Apply the embedded feedback CORS rules to the SDK and worker operations.
///
/// Other paths pass through. A request with an `Origin` (and its preflight)
/// is refused unless the `?project=` query names a project whose web
/// integrations register that origin; requests without one, from host
/// backends and workers, carry no CORS headers. The query project travels to
/// the operation, which refuses a different `project` argument.
pub async fn sdk_gate(
	State(state): State<Arc<AppState>>,
	mut request: Request,
	next: Next,
) -> Response {
	if !is_sdk_path(request.uri().path()) {
		return next.run(request).await;
	}

	let project = query_project(&request);
	if let Some(project) = project {
		request.extensions_mut().insert(QueryProject(project));
	}

	let mut response =
		monochange_app_api::embedded_api::cors_for_project(&state, project, request, next).await;
	let status = response.status();
	no_store(response.headers_mut(), status);
	response
}

fn no_store(headers: &mut HeaderMap, status: StatusCode) {
	headers.insert("cache-control", HeaderValue::from_static("no-store"));
	if status == StatusCode::TOO_MANY_REQUESTS {
		headers.insert("retry-after", HeaderValue::from_static("60"));
	}
}
