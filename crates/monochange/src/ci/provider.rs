//! External decision providers.
//!
//! Providers are advisory: every failure is turned into a `Result::Err` so the
//! caller can fall back to the deterministic workspace graph. Nothing in this
//! module panics on network, HTTP, or parse errors because a CI gate that
//! fails closed would block every pipeline behind a provider outage.
//!
//! Providers are data, not code paths: one preset table describes every
//! endpoint, one OpenAI-compatible client serves every chat provider, and one
//! request per `decide` run covers every job. Only `jev` keeps its own request
//! shape because its webhook wire format genuinely differs from chat
//! completions.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::Client;
use serde_json::Value;
use serde_json::json;

use super::CiDecisionProvider;

/// Connection timeout for provider requests.
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Whole-request timeout for provider requests.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Sampling temperature used for gatekeeper prompts.
const DETERMINISTIC_TEMPERATURE: f64 = 0.0;
/// Fallback account segment used when a Cloudflare account id is unknown.
const CLOUDFLARE_DEFAULT_ACCOUNT: &str = "default";
/// Default chat completions endpoint for the `opencode-go` provider.
pub const DEFAULT_OPENCODE_GO_API_URL: &str = "https://api.opencode.ai/v1/chat/completions";
/// Default model identifier for the `opencode-go` and `openai-compatible` providers.
pub const DEFAULT_OPENCODE_GO_MODEL: &str = "deepseek-v4.1";
/// Default chat completions endpoint for the `openrouter` provider.
pub const DEFAULT_OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
/// Default model identifier for the `openrouter` provider.
pub const DEFAULT_OPENROUTER_MODEL: &str = "deepseek/deepseek-chat";
/// Default Workers AI model identifier for the `cloudflare` provider.
pub const DEFAULT_CLOUDFLARE_MODEL: &str = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";
/// Default Cloudflare Workers AI chat completions URL template.
const CLOUDFLARE_API_URL_TEMPLATE: &str =
	"https://api.cloudflare.com/client/v4/accounts/{account}/ai/v1/chat/completions";
/// Model label recorded for the JEV webhook provider.
pub const JEV_MODEL_LABEL: &str = "jev-webhook";

/// How a provider preset resolves its endpoint URL.
#[derive(Debug, Clone, Copy)]
enum PresetUrl {
	/// A fixed chat-completions URL.
	Fixed(&'static str),
	/// A URL template containing `{account}`.
	Account(&'static str),
	/// No default; the URL must come from a flag or environment variable.
	Required,
}

/// Everything the shared client needs to know about one provider.
#[derive(Debug, Clone, Copy)]
struct ProviderPreset {
	/// Stable provider name used in requests, records, and CLI output.
	name: &'static str,
	/// Default endpoint resolution.
	url: PresetUrl,
	/// Environment variables probed for the endpoint URL, in order.
	url_env: &'static [&'static str],
	/// Environment variables probed for the API key, in order.
	api_key_env: &'static [&'static str],
	/// Whether the request fails when no API key resolves.
	requires_api_key: bool,
	/// Model used when neither `[ci]` model nor `--model` overrides it.
	default_model: &'static str,
}

/// Preset for the `opencode-go` provider.
const OPENCODE_GO_PRESET: ProviderPreset = ProviderPreset {
	name: "opencode-go",
	url: PresetUrl::Fixed(DEFAULT_OPENCODE_GO_API_URL),
	url_env: &["OPENCODE_GO_API_URL"],
	api_key_env: &["OPENCODE_GO_API_KEY"],
	requires_api_key: true,
	default_model: DEFAULT_OPENCODE_GO_MODEL,
};

/// Preset for the `openrouter` provider.
const OPENROUTER_PRESET: ProviderPreset = ProviderPreset {
	name: "openrouter",
	url: PresetUrl::Fixed(DEFAULT_OPENROUTER_API_URL),
	url_env: &["OPENROUTER_API_URL"],
	api_key_env: &["OPENROUTER_API_KEY"],
	requires_api_key: true,
	default_model: DEFAULT_OPENROUTER_MODEL,
};

/// Preset for the `cloudflare` provider, which embeds an account id.
const CLOUDFLARE_PRESET: ProviderPreset = ProviderPreset {
	name: "cloudflare",
	url: PresetUrl::Account(CLOUDFLARE_API_URL_TEMPLATE),
	url_env: &["CLOUDFLARE_API_URL"],
	api_key_env: &["CLOUDFLARE_API_TOKEN"],
	requires_api_key: true,
	default_model: DEFAULT_CLOUDFLARE_MODEL,
};

/// Preset for the `jev` webhook provider.
const JEV_PRESET: ProviderPreset = ProviderPreset {
	name: "jev",
	url: PresetUrl::Required,
	url_env: &["JEV_API_URL"],
	api_key_env: &["JEV_API_KEY"],
	requires_api_key: false,
	default_model: JEV_MODEL_LABEL,
};

/// Preset for a user-supplied OpenAI-compatible server.
const OPENAI_COMPATIBLE_PRESET: ProviderPreset = ProviderPreset {
	name: "openai-compatible",
	url: PresetUrl::Required,
	url_env: &["OPENAI_API_URL"],
	api_key_env: &["OPENAI_API_KEY"],
	requires_api_key: false,
	default_model: DEFAULT_OPENCODE_GO_MODEL,
};

/// Look up the preset for an external provider.
///
/// The local graph provider has no endpoint, so it is unrepresentable here:
/// `None` forces every caller to reject it instead of silently borrowing
/// another provider's endpoint.
fn preset_for(provider: CiDecisionProvider) -> Option<&'static ProviderPreset> {
	match provider {
		CiDecisionProvider::OpencodeGo => Some(&OPENCODE_GO_PRESET),
		CiDecisionProvider::Openrouter => Some(&OPENROUTER_PRESET),
		CiDecisionProvider::Cloudflare => Some(&CLOUDFLARE_PRESET),
		CiDecisionProvider::Jev => Some(&JEV_PRESET),
		CiDecisionProvider::OpenaiCompatible => Some(&OPENAI_COMPATIBLE_PRESET),
		CiDecisionProvider::Graph => None,
	}
}

/// Error returned when preset-dependent helpers receive the local graph.
const GRAPH_PROVIDER_ERROR: &str = "graph provider is local and has no endpoint";

/// Default model for an external provider when no override is configured.
pub(super) fn default_model_for(provider: CiDecisionProvider) -> Option<&'static str> {
	preset_for(provider).map(|preset| preset.default_model)
}

/// One job entry in a provider prompt.
pub(super) struct PromptJob<'a> {
	/// GitHub job id being decided.
	pub(super) id: &'a str,
	/// The deterministic graph verdict for this job.
	pub(super) graph_run: bool,
	/// Why the graph returned its verdict.
	pub(super) graph_reason: &'a str,
}

/// Everything about the change being classified, covering every job at once.
pub(super) struct DecisionPrompt<'a> {
	pub(super) jobs: &'a [PromptJob<'a>],
	pub(super) changed_files: &'a [String],
	pub(super) affected_packages: &'a [String],
	pub(super) recent_history: &'a [String],
}

/// CLI, `[ci]`, and environment overrides shared by every provider.
pub(super) struct ProviderOverrides<'a> {
	pub(super) model: Option<&'a str>,
	pub(super) api_url: Option<&'a str>,
	pub(super) api_key: Option<&'a str>,
	pub(super) account_id: Option<&'a str>,
}

/// A provider verdict for one job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProviderDecision {
	pub(super) run: bool,
	pub(super) reason: String,
}

/// Provider verdicts for every covered job plus the provider and model used.
#[derive(Debug)]
pub(super) struct ProviderCallOutcome {
	pub(super) decisions: BTreeMap<String, ProviderDecision>,
	pub(super) provider_used: String,
	pub(super) model_used: String,
}

/// Resolved chat-completions endpoint settings for one provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChatEndpoint {
	pub(super) url: String,
	pub(super) api_key: String,
	pub(super) default_model: &'static str,
}

/// Ask an external provider which jobs should run, with one request.
pub(super) async fn query_external_decision_provider(
	provider: CiDecisionProvider,
	overrides: &ProviderOverrides<'_>,
	prompt: &DecisionPrompt<'_>,
) -> Result<ProviderCallOutcome, String> {
	let client = build_http_client()?;

	match provider {
		CiDecisionProvider::Graph => Err("graph provider is local".to_string()),
		CiDecisionProvider::Jev => send_jev_request(&client, overrides, prompt).await,
		_ => send_chat_completion(&client, provider, overrides, prompt).await,
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

/// Send every job to a JEV webhook and parse the per-job decisions.
async fn send_jev_request(
	client: &Client,
	overrides: &ProviderOverrides<'_>,
	prompt: &DecisionPrompt<'_>,
) -> Result<ProviderCallOutcome, String> {
	let url = resolve_provider_url(CiDecisionProvider::Jev, overrides)?;
	let api_key = resolve_api_key(CiDecisionProvider::Jev, overrides);
	let mut request = client.post(url).json(&json!({
		"jobs": prompt.jobs.iter().map(|job| json!({
			"job": job.id,
			"graph": if job.graph_run { "y" } else { "n" },
			"reason": job.graph_reason,
		})).collect::<Vec<_>>(),
		"changed_files": prompt.changed_files,
		"affected_packages": prompt.affected_packages,
		"recent_history": prompt.recent_history,
	}));

	if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
		request = request.bearer_auth(key.trim());
	}

	let body = send_and_read(request, "jev webhook", "jev").await?;
	let decisions = parse_provider_decisions(&body)?;

	Ok(ProviderCallOutcome {
		decisions,
		provider_used: "jev".to_string(),
		model_used: overrides.model.unwrap_or(JEV_MODEL_LABEL).to_string(),
	})
}

/// Send every job in one OpenAI-compatible chat completion request.
async fn send_chat_completion(
	client: &Client,
	provider: CiDecisionProvider,
	overrides: &ProviderOverrides<'_>,
	prompt: &DecisionPrompt<'_>,
) -> Result<ProviderCallOutcome, String> {
	let preset = preset_for(provider).ok_or(GRAPH_PROVIDER_ERROR.to_string())?;
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

	let body = send_and_read(request, preset.name, "provider").await?;
	let decisions = parse_provider_decisions(&body)?;

	Ok(ProviderCallOutcome {
		decisions,
		provider_used: preset.name.to_string(),
		model_used: model,
	})
}

/// Build the chat-completions request body covering every job.
fn chat_request_body(model: &str, prompt: &DecisionPrompt<'_>) -> Value {
	let jobs = prompt
		.jobs
		.iter()
		.map(|job| {
			json!({
				"job": job.id,
				"graph": if job.graph_run { "y" } else { "n" },
				"reason": job.graph_reason,
			})
		})
		.collect::<Vec<_>>();

	json!({
		"model": model,
		"temperature": DETERMINISTIC_TEMPERATURE,
		"messages": [
			{
				"role": "system",
				"content": "You are monochange's CI gatekeeper. For each listed job decide whether it must run. Respond strictly with a JSON object mapping each job id to {\"run\": true|false, \"reason\": \"...\"} and no other text."
			},
			{
				"role": "user",
				"content": format!(
					"Jobs with graph verdicts: {jobs:?}. Changed files: {:?}. Affected workspace packages: {:?}. Recent history: {:?}.",
					prompt.changed_files, prompt.affected_packages, prompt.recent_history
				)
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

/// Resolve the endpoint URL for any provider from its preset.
pub(super) fn resolve_provider_url(
	provider: CiDecisionProvider,
	overrides: &ProviderOverrides<'_>,
) -> Result<String, String> {
	let Some(preset) = preset_for(provider) else {
		return Err(GRAPH_PROVIDER_ERROR.to_string());
	};
	let explicit = overrides
		.api_url
		.map(str::trim)
		.filter(|value| !value.is_empty())
		.map(ToString::to_string)
		.or_else(|| first_env_var(preset.url_env));

	match (preset.url, explicit) {
		(_, Some(url)) => Ok(url),
		(PresetUrl::Fixed(url), None) => Ok(url.to_string()),
		(PresetUrl::Account(template), None) => {
			let account = overrides
				.account_id
				.map(ToString::to_string)
				.or_else(|| env_var("CLOUDFLARE_ACCOUNT_ID"))
				.unwrap_or_else(|| CLOUDFLARE_DEFAULT_ACCOUNT.to_string());

			Ok(template.replace("{account}", &account))
		}
		(PresetUrl::Required, None) => {
			Err(format!(
				"missing --api-url or {} for {}",
				preset.url_env.first().map_or("an env var", |name| name),
				preset.name
			))
		}
	}
}

/// Resolve the API key for a provider from its preset.
fn resolve_api_key(
	provider: CiDecisionProvider,
	overrides: &ProviderOverrides<'_>,
) -> Option<String> {
	let preset = preset_for(provider)?;

	overrides
		.api_key
		.map(ToString::to_string)
		.or_else(|| first_env_var(preset.api_key_env))
}

/// Resolve endpoint settings for a chat-completions provider.
pub(super) fn resolve_chat_endpoint(
	provider: CiDecisionProvider,
	overrides: &ProviderOverrides<'_>,
) -> Result<ChatEndpoint, String> {
	let Some(preset) = preset_for(provider) else {
		return Err(GRAPH_PROVIDER_ERROR.to_string());
	};
	let url = resolve_provider_url(provider, overrides)?;
	let api_key = match resolve_api_key(provider, overrides) {
		Some(key) => key,
		None if !preset.requires_api_key => String::new(),
		None => {
			return Err(format!(
				"missing {} or --api-key",
				preset.api_key_env.first().map_or("an API key", |name| name)
			));
		}
	};

	Ok(ChatEndpoint {
		url,
		api_key,
		default_model: preset.default_model,
	})
}

/// Parse a provider payload into per-job decisions.
///
/// The expected shape is `{"<job>": {"run": bool, "reason": "..."}}`, either
/// directly (JEV and plain HTTP providers) or nested inside a chat completion
/// envelope. Models sometimes wrap JSON in code fences; strip them first.
pub(super) fn parse_provider_decisions(
	payload: &str,
) -> Result<BTreeMap<String, ProviderDecision>, String> {
	let trimmed = strip_code_fences(payload.trim());

	let value: Value =
		serde_json::from_str(trimmed).map_err(|error| format!("invalid JSON response: {error}"))?;

	if let Some(content) = value
		.pointer("/choices/0/message/content")
		.and_then(Value::as_str)
	{
		return parse_provider_decisions(content);
	}

	decisions_from_object(&value)
}

/// Remove Markdown code fences some models wrap around JSON.
fn strip_code_fences(payload: &str) -> &str {
	let without_prefix = payload
		.strip_prefix("```json")
		.or_else(|| payload.strip_prefix("```"));

	let body = without_prefix.unwrap_or(payload);
	let body = body.strip_suffix("```").unwrap_or(body);

	body.trim()
}

/// Read the per-job decisions out of a JSON object.
fn decisions_from_object(value: &Value) -> Result<BTreeMap<String, ProviderDecision>, String> {
	let Some(entries) = value.as_object() else {
		return Err("provider response was not a JSON object of job decisions".to_string());
	};

	let mut decisions = BTreeMap::new();

	for (job, entry) in entries {
		let decision = parse_decision_entry(entry).ok_or_else(|| {
			format!(
				"provider decision for `{job}` did not include `run` (`bool`) or `decision` (`y`/`n`)"
			)
		})?;

		decisions.insert(job.clone(), decision);
	}

	Ok(decisions)
}

/// Parse one job's decision entry.
fn parse_decision_entry(entry: &Value) -> Option<ProviderDecision> {
	let run = entry.get("run").and_then(Value::as_bool).or_else(|| {
		entry
			.get("decision")
			.and_then(Value::as_str)
			.map(|decision| matches_ignore_case(decision, &["y", "yes", "true", "run"]))
	})?;
	let reason = entry
		.get("reason")
		.and_then(Value::as_str)
		.map_or_else(|| default_decision_reason(run), ToString::to_string);

	Some(ProviderDecision { run, reason })
}

/// Default reason used when a provider omits one.
fn default_decision_reason(run: bool) -> String {
	if run {
		"provider decided to run job".to_string()
	} else {
		"provider decided to skip job".to_string()
	}
}

/// Compare a value against a set of case-insensitive labels.
fn matches_ignore_case(value: &str, candidates: &[&str]) -> bool {
	candidates
		.iter()
		.any(|candidate| value.eq_ignore_ascii_case(candidate))
}

/// Read the first non-empty environment variable from `names`.
fn first_env_var(names: &[&str]) -> Option<String> {
	names
		.iter()
		.find_map(|name| env_var(name).filter(|value| !value.trim().is_empty()))
}

/// Read a non-empty environment variable.
fn env_var(name: &str) -> Option<String> {
	std::env::var(name).ok()
}
