//! External decision providers.
//!
//! Providers are advisory: every failure is turned into a `Result::Err` so the
//! caller can fall back to the deterministic workspace graph. Nothing in this
//! module panics on network, HTTP, or parse errors because a CI gate that fails
//! closed would block every pipeline behind a provider outage.

use std::time::Duration;

use reqwest::Client;
use serde_json::Value;
use serde_json::json;

use super::CiDecisionProvider;
use super::DEFAULT_CLOUDFLARE_MODEL;
use super::DEFAULT_OPENCODE_GO_API_URL;
use super::DEFAULT_OPENCODE_GO_MODEL;
use super::DEFAULT_OPENROUTER_API_URL;
use super::DEFAULT_OPENROUTER_MODEL;

/// Connection timeout for provider requests.
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Whole-request timeout for provider requests.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Sampling temperature used for gatekeeper prompts.
const DETERMINISTIC_TEMPERATURE: f64 = 0.0;
/// Fallback account segment used when Cloudflare account id is unknown.
const CLOUDFLARE_DEFAULT_ACCOUNT: &str = "default";

/// Provider verdict plus the provider and model that produced it.
#[derive(Debug)]
pub(super) struct ProviderCallOutcome {
	pub(super) run: bool,
	pub(super) reason: String,
	pub(super) provider_used: String,
	pub(super) model_used: String,
}

/// Everything about the change being classified.
pub(super) struct DecisionPrompt<'a> {
	pub(super) job: &'a str,
	pub(super) changed_files: &'a [String],
	pub(super) affected_packages: &'a [String],
}

/// CLI and environment overrides shared by every provider.
pub(super) struct ProviderOverrides<'a> {
	pub(super) model: Option<&'a str>,
	pub(super) api_url: Option<&'a str>,
	pub(super) api_key: Option<&'a str>,
	pub(super) account_id: Option<&'a str>,
}

/// Resolved chat-completions endpoint settings for one provider.
pub(super) struct ChatEndpoint {
	pub(super) url: String,
	pub(super) api_key: String,
	pub(super) default_model: &'static str,
}

/// Providers that speak the `OpenAI` chat-completions protocol.
///
/// `graph` and `jev` are routed before this point, so the chat-completions
/// helpers only ever see these four variants.
#[derive(Debug, Clone, Copy)]
pub(super) enum ChatProvider {
	OpencodeGo,
	Openrouter,
	Cloudflare,
	OpenaiCompatible,
}

impl ChatProvider {
	/// Stable provider name used in requests, records, and CLI output.
	fn as_str(self) -> &'static str {
		match self {
			Self::OpencodeGo => "opencode-go",
			Self::Openrouter => "openrouter",
			Self::Cloudflare => "cloudflare",
			Self::OpenaiCompatible => "openai-compatible",
		}
	}
}

/// Ask an external provider whether the job should run.
pub(super) async fn query_external_decision_provider(
	provider: CiDecisionProvider,
	overrides: &ProviderOverrides<'_>,
	prompt: &DecisionPrompt<'_>,
) -> Result<ProviderCallOutcome, String> {
	let client = build_http_client()?;

	match provider {
		CiDecisionProvider::Graph => Err("graph provider is local".to_string()),
		CiDecisionProvider::Jev => send_jev_request(&client, overrides, prompt).await,
		CiDecisionProvider::OpencodeGo => {
			send_chat_completion(&client, ChatProvider::OpencodeGo, overrides, prompt).await
		}
		CiDecisionProvider::Openrouter => {
			send_chat_completion(&client, ChatProvider::Openrouter, overrides, prompt).await
		}
		CiDecisionProvider::Cloudflare => {
			send_chat_completion(&client, ChatProvider::Cloudflare, overrides, prompt).await
		}
		CiDecisionProvider::OpenaiCompatible => {
			send_chat_completion(&client, ChatProvider::OpenaiCompatible, overrides, prompt).await
		}
	}
}

/// Build the shared HTTP client with bounded timeouts.
fn build_http_client() -> Result<Client, String> {
	let _ = rustls::crypto::ring::default_provider().install_default();

	Client::builder()
		.connect_timeout(HTTP_CONNECT_TIMEOUT)
		.timeout(REQUEST_TIMEOUT)
		.build()
		.map_err(|error| format!("failed to build HTTP client: {error}"))
}

/// Send the job context to a JEV webhook and parse its decision.
async fn send_jev_request(
	client: &Client,
	overrides: &ProviderOverrides<'_>,
	prompt: &DecisionPrompt<'_>,
) -> Result<ProviderCallOutcome, String> {
	let url = resolve_jev_url(overrides)?;
	let api_key = overrides
		.api_key
		.map(ToString::to_string)
		.or_else(|| env_var("JEV_API_KEY"));

	let mut request = client.post(url).json(&json!({
		"job": prompt.job,
		"changed_files": prompt.changed_files,
		"affected_packages": prompt.affected_packages,
	}));

	if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
		request = request.bearer_auth(key.trim());
	}

	let body = send_and_read(request, "jev webhook", "jev").await?;
	let (run, reason) = parse_provider_decision_payload(&body)?;

	Ok(ProviderCallOutcome {
		run,
		reason,
		provider_used: "jev".to_string(),
		model_used: overrides.model.unwrap_or("jev-webhook").to_string(),
	})
}

/// Resolve the JEV endpoint from the flag or environment.
fn resolve_jev_url(overrides: &ProviderOverrides<'_>) -> Result<String, String> {
	overrides
		.api_url
		.map(str::trim)
		.filter(|value| !value.is_empty())
		.map(ToString::to_string)
		.or_else(|| {
			env_var("JEV_API_URL")
				.map(|value| value.trim().to_string())
				.filter(|value| !value.is_empty())
		})
		.ok_or_else(|| "missing JEV_API_URL or --api-url".to_string())
}

/// Send an OpenAI-compatible chat completion request and parse its decision.
async fn send_chat_completion(
	client: &Client,
	provider: ChatProvider,
	overrides: &ProviderOverrides<'_>,
	prompt: &DecisionPrompt<'_>,
) -> Result<ProviderCallOutcome, String> {
	let endpoint = resolve_chat_endpoint(provider, overrides)?;
	let model = overrides
		.model
		.unwrap_or(endpoint.default_model)
		.to_string();
	let mut request = client
		.post(&endpoint.url)
		.json(&chat_request_body(&model, prompt));

	if !endpoint.api_key.trim().is_empty() {
		request = request.bearer_auth(endpoint.api_key.trim());
	}

	let body = send_and_read(request, provider.as_str(), "provider").await?;
	let (run, reason) = parse_provider_decision_payload(&body)?;

	Ok(ProviderCallOutcome {
		run,
		reason,
		provider_used: provider.as_str().to_string(),
		model_used: model,
	})
}

/// Build the chat-completions request body for the gatekeeper prompt.
fn chat_request_body(model: &str, prompt: &DecisionPrompt<'_>) -> Value {
	let user_prompt = format!(
		"Decide if CI job `{}` must run (`y` or `n`). Changed files: {:?}. Affected workspace packages: {:?}. Reply with JSON {{\"decision\":\"y\"|\"n\",\"reason\":\"...\"}}.",
		prompt.job, prompt.changed_files, prompt.affected_packages
	);

	json!({
		"model": model,
		"temperature": DETERMINISTIC_TEMPERATURE,
		"messages": [
			{
				"role": "system",
				"content": "You are monochange's CI gatekeeper. Respond strictly with JSON: {\"decision\": \"y\" | \"n\", \"reason\": \"...\"}."
			},
			{
				"role": "user",
				"content": user_prompt
			}
		]
	})
}

/// Send a request and read the body, mapping transport and status errors.
async fn send_and_read(
	request: reqwest::RequestBuilder,
	request_label: &str,
	read_label: &str,
) -> Result<String, String> {
	let response = request
		.send()
		.await
		.map_err(|error| format!("{request_label} request failed: {error}"))?;

	if !response.status().is_success() {
		return Err(format!(
			"{request_label} returned HTTP {}",
			response.status()
		));
	}

	response
		.text()
		.await
		.map_err(|error| format!("failed reading {read_label} response: {error}"))
}

/// Resolve endpoint settings for one chat-completions provider.
pub(super) fn resolve_chat_endpoint(
	provider: ChatProvider,
	overrides: &ProviderOverrides<'_>,
) -> Result<ChatEndpoint, String> {
	match provider {
		ChatProvider::OpencodeGo => opencode_endpoint(overrides),
		ChatProvider::Openrouter => openrouter_endpoint(overrides),
		ChatProvider::Cloudflare => cloudflare_endpoint(overrides),
		ChatProvider::OpenaiCompatible => openai_compatible_endpoint(overrides),
	}
}

/// Endpoint settings for `opencode-go`.
fn opencode_endpoint(overrides: &ProviderOverrides<'_>) -> Result<ChatEndpoint, String> {
	let url = overrides
		.api_url
		.map(ToString::to_string)
		.or_else(|| env_var("OPENCODE_GO_API_URL"))
		.unwrap_or_else(|| DEFAULT_OPENCODE_GO_API_URL.to_string());
	let api_key = overrides
		.api_key
		.map(ToString::to_string)
		.or_else(|| env_var("OPENCODE_GO_API_KEY"))
		.ok_or_else(|| "missing OPENCODE_GO_API_KEY or --api-key".to_string())?;

	Ok(ChatEndpoint {
		url,
		api_key,
		default_model: DEFAULT_OPENCODE_GO_MODEL,
	})
}

/// Endpoint settings for `openrouter`.
fn openrouter_endpoint(overrides: &ProviderOverrides<'_>) -> Result<ChatEndpoint, String> {
	let url = overrides
		.api_url
		.map(ToString::to_string)
		.or_else(|| env_var("OPENROUTER_API_URL"))
		.unwrap_or_else(|| DEFAULT_OPENROUTER_API_URL.to_string());
	let api_key = overrides
		.api_key
		.map(ToString::to_string)
		.or_else(|| env_var("OPENROUTER_API_KEY"))
		.ok_or_else(|| "missing OPENROUTER_API_KEY or --api-key".to_string())?;

	Ok(ChatEndpoint {
		url,
		api_key,
		default_model: DEFAULT_OPENROUTER_MODEL,
	})
}

/// Endpoint settings for `cloudflare`, which embeds the account id in the URL.
fn cloudflare_endpoint(overrides: &ProviderOverrides<'_>) -> Result<ChatEndpoint, String> {
	let account_id = overrides
		.account_id
		.map(ToString::to_string)
		.or_else(|| env_var("CLOUDFLARE_ACCOUNT_ID"))
		.unwrap_or_else(|| CLOUDFLARE_DEFAULT_ACCOUNT.to_string());
	let url = overrides
		.api_url
		.map(ToString::to_string)
		.or_else(|| env_var("CLOUDFLARE_API_URL"))
		.unwrap_or_else(|| {
			format!(
				"https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1/chat/completions"
			)
		});
	let api_key = overrides
		.api_key
		.map(ToString::to_string)
		.or_else(|| env_var("CLOUDFLARE_API_TOKEN"))
		.ok_or_else(|| "missing CLOUDFLARE_API_TOKEN or --api-key".to_string())?;

	Ok(ChatEndpoint {
		url,
		api_key,
		default_model: DEFAULT_CLOUDFLARE_MODEL,
	})
}

/// Endpoint settings for a user-supplied OpenAI-compatible server.
fn openai_compatible_endpoint(overrides: &ProviderOverrides<'_>) -> Result<ChatEndpoint, String> {
	let url = overrides
		.api_url
		.map(ToString::to_string)
		.or_else(|| env_var("OPENAI_API_URL"))
		.ok_or_else(|| "missing --api-url or OPENAI_API_URL for openai-compatible".to_string())?;
	let api_key = overrides
		.api_key
		.map(ToString::to_string)
		.or_else(|| env_var("OPENAI_API_KEY"))
		.unwrap_or_default();

	Ok(ChatEndpoint {
		url,
		api_key,
		default_model: DEFAULT_OPENCODE_GO_MODEL,
	})
}

/// Parse a provider payload that may be plain text or a JSON chat envelope.
pub(super) fn parse_provider_decision_payload(payload: &str) -> Result<(bool, String), String> {
	let trimmed = payload.trim();

	if let Some(run) = parse_text_decision(trimmed) {
		let label = if run { "y" } else { "n" };

		return Ok((run, format!("provider returned `{label}`")));
	}

	let value: Value =
		serde_json::from_str(trimmed).map_err(|error| format!("invalid JSON response: {error}"))?;

	parse_json_decision(&value)
}

/// Parse bare `y` and `n` style responses.
fn parse_text_decision(trimmed: &str) -> Option<bool> {
	if matches_ignore_case(trimmed, &["y", "yes", "true"]) {
		return Some(true);
	}

	if matches_ignore_case(trimmed, &["n", "no", "false"]) {
		return Some(false);
	}

	None
}

/// Parse a JSON decision, unwrapping a nested chat completion when present.
fn parse_json_decision(value: &Value) -> Result<(bool, String), String> {
	if let Some(content) = value
		.pointer("/choices/0/message/content")
		.and_then(Value::as_str)
	{
		return parse_provider_decision_payload(content);
	}

	let run = json_decision_value(value).ok_or_else(|| {
		"provider response did not include `decision` (`y`/`n`) or `run` boolean".to_string()
	})?;
	let reason = value
		.get("reason")
		.and_then(Value::as_str)
		.unwrap_or_else(|| default_decision_reason(run))
		.to_string();

	Ok((run, reason))
}

/// Read the decision from either a `decision` string or a `run` boolean.
fn json_decision_value(value: &Value) -> Option<bool> {
	value
		.get("decision")
		.and_then(Value::as_str)
		.map(|decision| matches_ignore_case(decision, &["y", "yes", "true", "run"]))
		.or_else(|| value.get("run").and_then(Value::as_bool))
}

/// Default reason used when a provider omits one.
fn default_decision_reason(run: bool) -> &'static str {
	if run {
		"provider decided to run job"
	} else {
		"provider decided to skip job"
	}
}

/// Compare a value against a set of case-insensitive labels.
fn matches_ignore_case(value: &str, candidates: &[&str]) -> bool {
	candidates
		.iter()
		.any(|candidate| value.eq_ignore_ascii_case(candidate))
}

/// Read a non-empty environment variable.
fn env_var(name: &str) -> Option<String> {
	std::env::var(name).ok()
}
