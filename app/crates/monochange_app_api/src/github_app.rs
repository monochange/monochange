//! GitHub App authentication and API access.
//!
//! The monochange GitHub App is the bot identity that creates release commits
//! and release pull requests on connected repositories. Users install the app;
//! monochange keeps the app credentials and mints short-lived installation
//! tokens per repository. Installation tokens are never returned to callers.
//!
//! - App JWT: RS256 signed with the app private key, valid for 9 minutes.
//! - Installation tokens: minted from the app JWT, valid for 1 hour.
//! - Webhooks: HMAC-SHA256 verified with the app webhook secret.

use hmac::Hmac;
use hmac::Mac;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::encode;
use monochange_core::HostedCommitRequest;
use monochange_core::SourceChangeRequest;
use monochange_core::SourceChangeRequestOperation;
use serde::Deserialize;
use serde::Serialize;
use sha2::Sha256;
use thiserror::Error;

/// Errors raised while acting as the monochange GitHub App.
#[derive(Debug, Error)]
pub enum GitHubAppError {
	#[error(
		"GitHub App credentials are not configured: set GITHUB_APP_ID, GITHUB_APP_PRIVATE_KEY, and GITHUB_APP_WEBHOOK_SECRET"
	)]
	NotConfigured,
	#[error("GitHub App JWT signing failed: {0}")]
	Jwt(#[from] jsonwebtoken::errors::Error),
	#[error("GitHub API request failed: {0}")]
	Http(#[from] reqwest::Error),
	#[error("GitHub API `{0}` failed with status {1}: {2}")]
	Status(&'static str, reqwest::StatusCode, String),
	#[error(
		"release branch `{0}` moved to `{1}` while the release was being prepared; expected `{2}`. Re-run the release command."
	)]
	BranchMoved(String, String, String),
	#[error("GitHub reported the commit as unverified")]
	UnverifiedCommit,
	#[error("invalid webhook signature")]
	WebhookSignature,
}

/// Monochange GitHub App credentials resolved from the environment.
#[derive(Debug, Clone)]
pub struct GitHubAppAuth {
	pub app_id: String,
	private_key: String,
	webhook_secret: String,
	/// API base URL; `https://api.github.com` unless overridden for GitHub Enterprise.
	pub api_url: String,
}

impl GitHubAppAuth {
	/// Build app credentials from explicit values; tests use this to verify
	/// signing and webhook behavior without touching the environment.
	#[must_use]
	pub fn new(app_id: &str, private_key: &str, webhook_secret: &str, api_url: &str) -> Self {
		Self {
			app_id: app_id.to_string(),
			private_key: private_key.to_string(),
			webhook_secret: webhook_secret.to_string(),
			api_url: api_url.trim_end_matches('/').to_string(),
		}
	}

	/// Resolve the app credentials from environment variables.
	///
	/// Returns `None` when the app is not configured, so local development and
	/// CI can run the website without the bot endpoints.
	#[must_use]
	pub fn from_env() -> Option<Self> {
		let app_id = std::env::var("GITHUB_APP_ID").ok()?;
		let private_key = std::env::var("GITHUB_APP_PRIVATE_KEY").ok()?;
		let webhook_secret = std::env::var("GITHUB_APP_WEBHOOK_SECRET").ok()?;
		if app_id.is_empty() || private_key.is_empty() {
			return None;
		}
		let api_url = std::env::var("GITHUB_API_URL")
			.ok()
			.filter(|url| !url.is_empty())
			.unwrap_or_else(|| "https://api.github.com".to_string());
		Some(Self {
			app_id,
			private_key,
			webhook_secret,
			api_url: api_url.trim_end_matches('/').to_string(),
		})
	}

	/// Whether webhook verification is configured.
	#[must_use]
	pub fn webhook_configured(&self) -> bool {
		!self.webhook_secret.is_empty()
	}

	/// Verify a `X-Hub-Signature-256` header against the raw webhook payload.
	pub fn verify_webhook_signature(
		&self,
		payload: &[u8],
		signature: &str,
	) -> Result<(), GitHubAppError> {
		let expected = webhook_signature(&self.webhook_secret, payload);
		if constant_time_eq(expected.as_bytes(), signature.as_bytes()) {
			Ok(())
		} else {
			Err(GitHubAppError::WebhookSignature)
		}
	}

	/// Sign the short-lived GitHub App JWT used to mint installation tokens.
	fn app_jwt(&self) -> Result<String, GitHubAppError> {
		let now = chrono::Utc::now().timestamp();
		let claims = serde_json::json!({
			"iat": now,
			"exp": now + (9 * 60),
			"iss": self.app_id,
		});
		let key =
			EncodingKey::from_rsa_pem(self.private_key.as_bytes()).map_err(GitHubAppError::Jwt)?;
		encode(&Header::new(jsonwebtoken::Algorithm::RS256), &claims, &key)
			.map_err(GitHubAppError::Jwt)
	}

	/// Mint a one-hour installation token for one GitHub App installation.
	pub async fn installation_token(
		&self,
		http: &reqwest::Client,
		installation_id: i64,
	) -> Result<String, GitHubAppError> {
		let jwt = self.app_jwt()?;
		let url = format!(
			"{}/app/installations/{installation_id}/access_tokens",
			self.api_url
		);
		let response = http
			.post(url)
			.bearer_auth(jwt)
			.header("Accept", "application/vnd.github+json")
			.send()
			.await?;
		if !response.status().is_success() {
			let status = response.status();
			let body = response.text().await.unwrap_or_default();
			return Err(GitHubAppError::Status(
				"create installation token",
				status,
				body,
			));
		}
		let token: InstallationTokenResponse = response.json().await?;
		Ok(token.token)
	}
}

/// Compare two byte strings without early exit.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
	if left.len() != right.len() {
		return false;
	}
	left.iter()
		.zip(right)
		.fold(0u8, |acc, (a, b)| acc | (a ^ b))
		== 0
}

/// Compute the `sha256=` HMAC webhook signature for a payload.
fn webhook_signature(secret: &str, payload: &[u8]) -> String {
	let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
		.unwrap_or_else(|error| panic!("HMAC accepts any key length: {error}"));
	mac.update(payload);
	format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[derive(Debug, Deserialize)]
struct InstallationTokenResponse {
	token: String,
}

#[derive(Debug, Serialize)]
struct BlobRequest<'a> {
	content: &'a str,
	encoding: &'static str,
}

#[derive(Debug, Serialize)]
struct TreeEntry {
	path: String,
	mode: String,
	#[serde(rename = "type")]
	kind: String,
	sha: Option<String>,
}

#[derive(Debug, Serialize)]
struct TreeRequest {
	base_tree: String,
	tree: Vec<TreeEntry>,
}

#[derive(Debug, Deserialize)]
struct TreeResponse {
	sha: String,
}

#[derive(Debug, Serialize)]
struct CommitRequest<'a> {
	message: String,
	tree: &'a str,
	parents: Vec<&'a str>,
}

#[derive(Debug, Deserialize)]
struct CommitResponse {
	sha: String,
	#[serde(default)]
	verification: Option<CommitVerification>,
}

#[derive(Debug, Deserialize)]
struct CommitVerification {
	#[serde(default)]
	verified: bool,
	#[serde(default)]
	reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RefResponse {
	object: RefObject,
}

#[derive(Debug, Deserialize)]
struct RefObject {
	sha: String,
}

#[derive(Debug, Serialize)]
struct UpdateRefRequest<'a> {
	sha: &'a str,
	force: bool,
}

#[derive(Debug, Serialize)]
struct CreateRefRequest<'a> {
	#[serde(rename = "ref")]
	reference: String,
	sha: &'a str,
}

#[derive(Debug, Deserialize)]
struct PullRequestResponse {
	number: u64,
	html_url: String,
	#[serde(default)]
	node_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreatePullRequestRequest<'a> {
	title: &'a str,
	head: &'a str,
	base: &'a str,
	body: &'a str,
}

#[derive(Debug, Serialize)]
struct UpdatePullRequestRequest<'a> {
	title: &'a str,
	body: &'a str,
}

#[derive(Debug, Serialize)]
struct LabelRequest {
	labels: Vec<String>,
}

/// A repository listed by an installation.
#[derive(Debug, Clone, Deserialize)]
pub struct InstallationRepository {
	pub id: i64,
	pub name: String,
	pub full_name: String,
	#[serde(default)]
	pub private: bool,
}

#[derive(Debug, Deserialize)]
struct InstallationRepositoriesResponse {
	#[serde(default)]
	repositories: Vec<InstallationRepository>,
}

/// List every repository an installation token can access.
pub async fn list_installation_repositories(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
) -> Result<Vec<InstallationRepository>, GitHubAppError> {
	let response = http
		.get(format!("{api_url}/installation/repositories?per_page=100"))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.send()
		.await?;
	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(GitHubAppError::Status(
			"list installation repositories",
			status,
			body,
		));
	}
	let page: InstallationRepositoriesResponse = response.json().await?;
	Ok(page.repositories)
}

fn api_error(operation: &'static str, status: reqwest::StatusCode, body: String) -> GitHubAppError {
	GitHubAppError::Status(operation, status, body)
}

/// Resolve the commit parent for the release branch.
///
/// When the branch exists, its head must equal the prepared base commit so a
/// rerun never overwrites newer work. When the branch is missing (first run),
/// the prepared base commit becomes both the parent and the new ref.
async fn resolve_release_branch_head(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	owner: &str,
	repo: &str,
	branch: &str,
	base_commit: &str,
) -> Result<BranchHead, GitHubAppError> {
	let response = http
		.get(format!(
			"{api_url}/repos/{owner}/{repo}/git/ref/heads/{branch}"
		))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.send()
		.await?;
	if response.status().as_u16() == 404 {
		return Ok(BranchHead::Missing);
	}
	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(api_error("read release branch ref", status, body));
	}
	let reference: RefResponse = response.json().await?;
	if reference.object.sha != base_commit {
		return Err(GitHubAppError::BranchMoved(
			branch.to_string(),
			reference.object.sha,
			base_commit.to_string(),
		));
	}
	Ok(BranchHead::AtBase)
}

enum BranchHead {
	AtBase,
	Missing,
}

/// Create the release commit through the GitHub Git Database API.
///
/// Commits created by a GitHub App with the Git Database API are signed by
/// GitHub and report `verification.verified = true`, which satisfies
/// verified-commit branch protection and triggers repository workflows.
pub async fn create_release_commit(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	request: &HostedCommitRequest,
) -> Result<(String, bool, Option<String>), GitHubAppError> {
	let owner = &request.owner;
	let repo = &request.repository;
	let base_commit = &request.base_commit;

	// 1. Blobs for every file with content.
	let mut tree = Vec::new();
	for file in &request.files {
		match &file.content {
			Some(content) => {
				let response = http
					.post(format!("{api_url}/repos/{owner}/{repo}/git/blobs"))
					.bearer_auth(installation_token)
					.header("Accept", "application/vnd.github+json")
					.json(&BlobRequest {
						content,
						encoding: "utf-8",
					})
					.send()
					.await?;
				if !response.status().is_success() {
					let status = response.status();
					let body = response.text().await.unwrap_or_default();
					return Err(api_error("create blob", status, body));
				}
				let blob: TreeResponse = response.json().await?;
				tree.push(TreeEntry {
					path: file.path.clone(),
					mode: "100644".to_string(),
					kind: "blob".to_string(),
					sha: Some(blob.sha),
				});
			}
			None => {
				// A missing content deletes the path; GitHub expects an
				// explicit null sha for deletions inside a tree.
				tree.push(TreeEntry {
					path: file.path.clone(),
					mode: "100644".to_string(),
					kind: "blob".to_string(),
					sha: None,
				});
			}
		}
	}

	// 2. Guard against a moved branch before writing anything.
	let head = resolve_release_branch_head(
		http,
		api_url,
		installation_token,
		owner,
		repo,
		&request.branch,
		base_commit,
	)
	.await?;

	// 3. Tree on top of the prepared base commit.
	let response = http
		.post(format!("{api_url}/repos/{owner}/{repo}/git/trees"))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.json(&TreeRequest {
			base_tree: base_commit.clone(),
			tree,
		})
		.send()
		.await?;
	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(api_error("create tree", status, body));
	}
	let tree_response: TreeResponse = response.json().await?;

	// 4. Commit with the prepared base as its only parent.
	let message = if request.body.is_empty() {
		request.subject.clone()
	} else {
		format!("{}\n\n{}", request.subject, request.body)
	};
	let response = http
		.post(format!("{api_url}/repos/{owner}/{repo}/git/commits"))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.json(&CommitRequest {
			message,
			tree: &tree_response.sha,
			parents: vec![base_commit],
		})
		.send()
		.await?;
	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(api_error("create commit", status, body));
	}
	let commit: CommitResponse = response.json().await?;
	let verified = commit
		.verification
		.as_ref()
		.is_some_and(|verification| verification.verified);
	let verification_reason = commit
		.verification
		.as_ref()
		.and_then(|verification| verification.reason.clone());
	if !verified {
		tracing::warn!(
			commit = %commit.sha,
			reason = verification_reason.as_deref().unwrap_or("unknown"),
			"GitHub did not verify the hosted release commit"
		);
	}

	// 5. Point the release branch at the new commit. The first run creates
	// the branch; later runs move it forward from the verified base.
	match head {
		BranchHead::Missing => {
			let response = http
				.post(format!("{api_url}/repos/{owner}/{repo}/git/refs"))
				.bearer_auth(installation_token)
				.header("Accept", "application/vnd.github+json")
				.json(&CreateRefRequest {
					reference: format!("refs/heads/{}", request.branch),
					sha: &commit.sha,
				})
				.send()
				.await?;
			if !response.status().is_success() {
				let status = response.status();
				let body = response.text().await.unwrap_or_default();
				return Err(api_error("create release branch ref", status, body));
			}
		}
		BranchHead::AtBase => {
			let response = http
				.patch(format!(
					"{api_url}/repos/{owner}/{repo}/git/refs/heads/{}",
					request.branch
				))
				.bearer_auth(installation_token)
				.header("Accept", "application/vnd.github+json")
				.json(&UpdateRefRequest {
					sha: &commit.sha,
					force: false,
				})
				.send()
				.await?;
			if !response.status().is_success() {
				let status = response.status();
				let body = response.text().await.unwrap_or_default();
				return Err(api_error("update release branch ref", status, body));
			}
		}
	}

	Ok((commit.sha, verified, verification_reason))
}

/// Look up the open pull request for a head branch.
async fn find_open_pull_request(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	owner: &str,
	repo: &str,
	head_branch: &str,
) -> Result<Option<PullRequestResponse>, GitHubAppError> {
	let response = http
		.get(format!(
			"{api_url}/repos/{owner}/{repo}/pulls?state=open&head={owner}%3A{head_branch}"
		))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.send()
		.await?;
	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(api_error("list pull requests", status, body));
	}
	let pulls: Vec<PullRequestResponse> = response.json().await?;
	Ok(pulls.into_iter().next())
}

/// Open or update the release pull request under the bot identity.
///
/// Returns the pull request number, the performed operation, and its URL.
pub async fn publish_release_pull_request(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	request: &SourceChangeRequest,
) -> Result<(u64, SourceChangeRequestOperation, String), GitHubAppError> {
	let owner = &request.owner;
	let repo = &request.repo;

	let existing = find_open_pull_request(
		http,
		api_url,
		installation_token,
		owner,
		repo,
		&request.head_branch,
	)
	.await?;

	let (pull, operation) = if let Some(existing) = existing {
		let response = http
			.patch(format!(
				"{api_url}/repos/{owner}/{repo}/pulls/{}",
				existing.number
			))
			.bearer_auth(installation_token)
			.header("Accept", "application/vnd.github+json")
			.json(&UpdatePullRequestRequest {
				title: &request.title,
				body: &request.body,
			})
			.send()
			.await?;
		if !response.status().is_success() {
			let status = response.status();
			let body = response.text().await.unwrap_or_default();
			return Err(api_error("update pull request", status, body));
		}
		let pull: PullRequestResponse = response.json().await?;
		(pull, SourceChangeRequestOperation::Updated)
	} else {
		let response = http
			.post(format!("{api_url}/repos/{owner}/{repo}/pulls"))
			.bearer_auth(installation_token)
			.header("Accept", "application/vnd.github+json")
			.json(&CreatePullRequestRequest {
				title: &request.title,
				head: &request.head_branch,
				base: &request.base_branch,
				body: &request.body,
			})
			.send()
			.await?;
		if !response.status().is_success() {
			let status = response.status();
			let body = response.text().await.unwrap_or_default();
			return Err(api_error("create pull request", status, body));
		}
		let pull: PullRequestResponse = response.json().await?;
		(pull, SourceChangeRequestOperation::Created)
	};

	if !request.labels.is_empty() {
		let response = http
			.post(format!(
				"{api_url}/repos/{owner}/{repo}/issues/{}/labels",
				pull.number
			))
			.bearer_auth(installation_token)
			.header("Accept", "application/vnd.github+json")
			.json(&LabelRequest {
				labels: request.labels.clone(),
			})
			.send()
			.await?;
		if !response.status().is_success() {
			let status = response.status();
			let body = response.text().await.unwrap_or_default();
			return Err(api_error("label pull request", status, body));
		}
	}

	if request.auto_merge
		&& let Some(node_id) = pull.node_id.as_deref()
	{
		let _ = enable_pull_request_auto_merge(http, api_url, installation_token, node_id).await;
	}

	Ok((pull.number, operation, pull.html_url))
}

/// Enable auto-merge on a pull request through the GraphQL API.
pub async fn enable_pull_request_auto_merge(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	pull_request_node_id: &str,
) -> Result<bool, GitHubAppError> {
	let query = r"
	mutation($pullRequestId: ID!) {
		enablePullRequestAutoMerge(input: { pullRequestId: $pullRequestId }) {
			pullRequest { number }
		}
	}";
	let response = http
		.post(format!("{api_url}/graphql"))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.json(&serde_json::json!({
			"query": query,
			"variables": { "pullRequestId": pull_request_node_id },
		}))
		.send()
		.await?;
	Ok(response.status().is_success())
}
