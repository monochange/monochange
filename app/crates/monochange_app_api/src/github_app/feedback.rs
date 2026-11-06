//! Creates draft feedback PRs from bounded, reviewed text changes.

use reqwest::Method;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

use super::GitHubAppError;
use super::api_error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackFile {
	pub path: String,
	#[serde(default)]
	pub executable: bool,
	/// `None` deletes a regular file. Binary and symlink changes are not accepted.
	pub content: Option<String>,
}

pub struct FeedbackPullRequest<'a> {
	pub repository: &'a str,
	pub base: &'a str,
	pub branch: &'a str,
	pub title: &'a str,
	pub body: &'a str,
	pub marker: &'a str,
	pub files: &'a [FeedbackFile],
}

async fn send(
	http: &reqwest::Client,
	method: Method,
	url: &str,
	token: &str,
	body: Option<Value>,
) -> Result<Value, GitHubAppError> {
	let mut request = http
		.request(method, url)
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json");

	if let Some(body) = body {
		request = request.json(&body);
	}

	let response = request.send().await?;

	if !response.status().is_success() {
		return Err(api_error(
			"feedback pull request",
			response.status(),
			response.text().await.unwrap_or_default(),
		));
	}

	Ok(response.json().await?)
}

fn field(value: &Value, name: &str) -> Result<String, GitHubAppError> {
	value
		.get(name)
		.and_then(Value::as_str)
		.map(str::to_owned)
		.ok_or(GitHubAppError::UnboundPullRequest)
}

/// Reconciliation compares the exact tree and parent before reusing a branch.
pub async fn create_feedback_pull_request(
	http: &reqwest::Client,
	api: &str,
	token: &str,
	request: &FeedbackPullRequest<'_>,
) -> Result<(u64, String), GitHubAppError> {
	let root = format!("{}/repos/{}", api.trim_end_matches('/'), request.repository);

	if !request.branch.starts_with("monochange/feedback/") || request.files.is_empty() {
		return Err(GitHubAppError::UnboundPullRequest);
	}

	let repository = send(http, Method::GET, &root, token, None).await?;
	let default_branch = field(&repository, "default_branch")?;
	let base = send(
		http,
		Method::GET,
		&format!("{root}/git/commits/{}", request.base),
		token,
		None,
	)
	.await?;
	let base_tree = field(
		base.get("tree").ok_or(GitHubAppError::UnboundPullRequest)?,
		"sha",
	)?;
	let tree: Vec<Value> = request
		.files
		.iter()
		.map(|file| {
			match &file.content {
				Some(content) => {
					json!({"path":file.path,"mode":if file.executable {"100755"} else {"100644"},"type":"blob","content":content})
				}
				None => json!({"path":file.path,"mode":"100644","type":"blob","sha":null}),
			}
		})
		.collect();
	let tree = send(
		http,
		Method::POST,
		&format!("{root}/git/trees"),
		token,
		Some(json!({"base_tree":base_tree,"tree":tree})),
	)
	.await?;
	let tree_sha = field(&tree, "sha")?;
	let ref_url = format!("{root}/git/ref/heads/{}", request.branch);
	let reference = http.get(&ref_url).bearer_auth(token).send().await?;
	let commit_sha = if reference.status() == reqwest::StatusCode::NOT_FOUND {
		let commit = send(http, Method::POST, &format!("{root}/git/commits"), token, Some(json!({"message":format!("{}\n\n{}\n\n{}",request.title,request.body,request.marker),"tree":tree_sha,"parents":[request.base]}))).await?;
		let sha = field(&commit, "sha")?;

		if !commit
			.get("verification")
			.and_then(|verification| verification.get("verified"))
			.and_then(Value::as_bool)
			.unwrap_or(false)
		{
			return Err(GitHubAppError::UnverifiedCommit(
				sha,
				"feedback commit is not verified".into(),
			));
		}
		send(
			http,
			Method::POST,
			&format!("{root}/git/refs"),
			token,
			Some(json!({"ref":format!("refs/heads/{}",request.branch),"sha":sha})),
		)
		.await?;
		sha
	} else {
		if !reference.status().is_success() {
			return Err(api_error(
				"read feedback branch",
				reference.status(),
				reference.text().await.unwrap_or_default(),
			));
		}

		let reference: Value = reference.json().await?;
		let sha = field(
			reference
				.get("object")
				.ok_or(GitHubAppError::UnboundPullRequest)?,
			"sha",
		)?;
		let commit = send(
			http,
			Method::GET,
			&format!("{root}/git/commits/{sha}"),
			token,
			None,
		)
		.await?;

		if commit.pointer("/tree/sha").and_then(Value::as_str) != Some(&tree_sha)
			|| commit.pointer("/parents/0/sha").and_then(Value::as_str) != Some(request.base)
			|| !commit
				.get("message")
				.and_then(Value::as_str)
				.is_some_and(|message| message.ends_with(request.marker))
		{
			return Err(GitHubAppError::UnboundPullRequest);
		}
		sha
	};

	let owner = request
		.repository
		.split('/')
		.next()
		.ok_or(GitHubAppError::UnboundPullRequest)?;
	let response = http
		.get(format!("{root}/pulls"))
		.bearer_auth(token)
		.query(&[
			("head", format!("{owner}:{}", request.branch)),
			("state", "open".into()),
		])
		.send()
		.await?;

	if !response.status().is_success() {
		return Err(api_error(
			"find feedback pull request",
			response.status(),
			response.text().await.unwrap_or_default(),
		));
	}

	let pulls: Vec<Value> = response.json().await?;
	let pull = if let Some(pull) = pulls.into_iter().next() {
		if pull.pointer("/head/sha").and_then(Value::as_str) != Some(&commit_sha)
			|| pull.pointer("/head/ref").and_then(Value::as_str) != Some(request.branch)
			|| pull.pointer("/base/ref").and_then(Value::as_str) != Some(&default_branch)
			|| !pull
				.get("body")
				.and_then(Value::as_str)
				.is_some_and(|body| body.ends_with(request.marker))
		{
			return Err(GitHubAppError::UnboundPullRequest);
		}
		pull
	} else {
		send(http, Method::POST, &format!("{root}/pulls"), token, Some(json!({"title":request.title,"head":request.branch,"base":default_branch,"body":format!("{}\n\n{}",request.body,request.marker),"draft":true}))).await?
	};

	let number = pull
		.get("number")
		.and_then(Value::as_u64)
		.ok_or(GitHubAppError::UnboundPullRequest)?;
	Ok((number, field(&pull, "html_url")?))
}

#[cfg(test)]
#[path = "__tests__/feedback_tests.rs"]
mod tests;
