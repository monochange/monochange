//! `monochange gh workflow run` — dispatch workflows through the monochange app.
//!
//! The command mirrors the useful part of `gh workflow run`, but authenticates
//! to the monochange app with GitHub Actions OIDC (or a local
//! `MONOCHANGE_TOKEN`) and lets the app use its GitHub App installation token.
//! An OIDC caller may dispatch to any repository in the same installation, so
//! an organisation repository can trigger a release workflow in a sibling
//! repository without a personal access token.

#[cfg(test)]
#[path = "__tests__/workflow_dispatch_tests.rs"]
mod tests;

use std::collections::BTreeMap;
use std::time::Duration;

use clap::ArgMatches;
use monochange_core::HostedCommitAuth;
use monochange_core::HostedWorkflowDispatchRequest;
use monochange_core::HostedWorkflowDispatchResponse;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

use crate::OutputFormat;
use crate::parse_output_format;
use crate::release_artifacts::HostedCommitOptions;
use crate::release_artifacts::hosted_app_bearer_token;
use crate::release_artifacts::resolve_hosted_commit_options;

const WORKFLOW_DISPATCH_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) async fn run(matches: &ArgMatches) -> MonochangeResult<String> {
	let workflow = matches
		.get_one::<String>("workflow")
		.cloned()
		.ok_or_else(|| MonochangeError::Config("missing workflow name".to_string()))?;
	let repository = matches
		.get_one::<String>("repo")
		.cloned()
		.ok_or_else(|| MonochangeError::Config("missing --repo value".to_string()))?;
	let (owner, name) = split_repository(&repository)?;
	let git_ref = matches
		.get_one::<String>("ref")
		.cloned()
		.unwrap_or_else(|| "main".to_string());
	let inputs = parse_fields(matches)?;
	let dry_run = matches.get_flag("dry-run");
	let format = matches
		.get_one::<String>("format")
		.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;
	let auth = matches
		.get_one::<String>("hosted-auth")
		.map_or(Ok(HostedCommitAuth::Auto), |value| value.parse())?;
	let options = resolve_hosted_commit_options(
		auth,
		matches.get_one::<String>("hosted-url").map(String::as_str),
		matches
			.get_one::<String>("oidc-audience")
			.map(String::as_str),
	);

	let request = HostedWorkflowDispatchRequest {
		provider: "github".to_string(),
		owner,
		repository: name,
		workflow,
		git_ref,
		inputs,
		dry_run,
	};
	let token = hosted_app_bearer_token(&options, "workflow dispatch").await?;
	let response = send_workflow_dispatch(&request, &options, &token).await?;

	render_workflow_dispatch(&request, &response, format)
}

fn split_repository(repository: &str) -> MonochangeResult<(String, String)> {
	let (owner, name) = repository.split_once('/').ok_or_else(|| {
		MonochangeError::Config(format!(
			"`--repo {repository}` must use `owner/repo` format"
		))
	})?;

	if owner.is_empty() || name.is_empty() || name.contains('/') {
		return Err(MonochangeError::Config(format!(
			"`--repo {repository}` must use `owner/repo` format"
		)));
	}

	Ok((owner.to_string(), name.to_string()))
}

fn parse_fields(matches: &ArgMatches) -> MonochangeResult<BTreeMap<String, String>> {
	let mut fields = BTreeMap::new();

	for field in matches.get_many::<String>("field").into_iter().flatten() {
		let (name, value) = field.split_once('=').ok_or_else(|| {
			MonochangeError::Config(format!(
				"workflow input `{field}` must use `KEY=VALUE` format"
			))
		})?;

		if name.is_empty() {
			return Err(MonochangeError::Config(format!(
				"workflow input `{field}` has an empty name"
			)));
		}

		if fields.insert(name.to_string(), value.to_string()).is_some() {
			return Err(MonochangeError::Config(format!(
				"workflow input `{name}` was provided more than once"
			)));
		}
	}

	Ok(fields)
}

async fn send_workflow_dispatch(
	request: &HostedWorkflowDispatchRequest,
	options: &HostedCommitOptions,
	token: &str,
) -> MonochangeResult<HostedWorkflowDispatchResponse> {
	let client = monochange_hosting::build_http_client("monochange app")?;
	let response = client
		.post(format!(
			"{}/api/workflow-dispatches",
			options.url.trim_end_matches('/')
		))
		.timeout(WORKFLOW_DISPATCH_TIMEOUT)
		.bearer_auth(token)
		.json(request)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("hosted workflow dispatch request failed: {error}"))
		})?;
	let status = response.status();
	// patch-coverage:ignore-start -- a mid-body connection drop is needed to fail the text read; httpmock always serves complete bodies.
	let text = response.text().await.map_err(|error| {
		MonochangeError::Config(format!(
			"hosted workflow dispatch response read failed: {error}"
		))
	})?;
	// patch-coverage:ignore-end
	if !status.is_success() {
		return Err(MonochangeError::Config(format!(
			"hosted workflow dispatch failed with HTTP {status}: {text}"
		)));
	}

	serde_json::from_str(&text).map_err(|error| {
		MonochangeError::Config(format!(
			"hosted workflow dispatch response was invalid JSON: {error}"
		))
	})
}

fn render_workflow_dispatch(
	request: &HostedWorkflowDispatchRequest,
	response: &HostedWorkflowDispatchResponse,
	format: OutputFormat,
) -> MonochangeResult<String> {
	let repository = format!("{}/{}", request.owner, request.repository);

	match format {
		OutputFormat::Json | OutputFormat::JsonMin => {
			format.render_json_value(response, "workflow dispatch")
		}
		OutputFormat::Markdown => {
			Ok(format!(
				"## Workflow dispatch\n\n| Repository | Workflow | Ref | Status |\n| --- | --- | --- | --- |\n| `{repository}` | `{}` | `{}` | `{}` |\n",
				request.workflow,
				request.git_ref,
				response.status.as_deref().unwrap_or("unknown")
			))
		}
		OutputFormat::Text => {
			let verb = if request.dry_run {
				"Would dispatch"
			} else {
				"Dispatched"
			};
			Ok(format!(
				"{verb} {repository} {} on {} via monochange app",
				request.workflow, request.git_ref
			))
		}
	}
}
