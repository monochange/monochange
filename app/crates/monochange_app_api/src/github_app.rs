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

#[cfg(test)]
#[path = "__tests__/github_app_tests.rs"]
mod tests;

pub mod feedback;

use std::collections::BTreeMap;

use hmac::Hmac;
use hmac::Mac;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::encode;
use monochange_core::HostedCommitFile;
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
	#[error(
		"base branch `{0}` moved to `{1}` while the release was being prepared from `{2}`; the run for the newer commit refreshes the release branch"
	)]
	BaseMoved(String, String, String),
	#[error(
		"GitHub created the release commit `{0}` but reported it as unverified ({1}); the release branch was not moved"
	)]
	UnverifiedCommit(String, String),
	#[error(
		"the release tree changes `.github/workflows`; release commits must not modify GitHub workflows"
	)]
	WorkflowsChanged,
	#[error("branch `{0}` must be a release branch distinct from the base and default branches")]
	InvalidReleaseBranch(String),
	#[error(
		"frozen cut `{0}` requires a named `base_branch` so its base commit can be ancestry-checked"
	)]
	CutBaseRequiresBase(String),
	#[error(
		"frozen cut `{0}` cannot use release branch `{1}` as its source base; name the normal branch the cut was prepared from"
	)]
	InvalidCutBase(String, String),
	#[error(
		"frozen cut base branch `{0}` is at `{1}` and cannot be replaced from `{2}`; only a fast-forward or a verified same-cut refresh may move it"
	)]
	CutBaseConflict(String, String, String),
	#[error(
		"pull request is not a verified monochange release request for this head, base, and repository: it was not created by this monochange deployment's verified flow (for example, it predates release-PR verification, was opened by a user or another bot, or the deployment's signing secret changed). Close the existing pull request once and re-run the release workflow so monochange opens a verified one."
	)]
	UnboundPullRequest,
	#[error(
		"release pull request {url} was created but could not be authenticated; close this newly created release PR and retry: {source}"
	)]
	ReleasePullRequestNeedsRecovery {
		url: String,
		#[source]
		source: Box<GitHubAppError>,
	},
	#[error("invalid webhook signature")]
	WebhookSignature,
	#[error("invalid GitHub app slug")]
	InvalidAppSlug,
	#[error(
		"JWT signing secret is empty; set JWT_SECRET to a non-empty value before starting the server"
	)]
	EmptyJwtSecret,
	#[error(
		"token encryption key is empty; set TOKEN_ENCRYPTION_KEY to a non-empty value before starting the server"
	)]
	EmptyTokenEncryptionKey,
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

	/// Resolve the configured app's repository installation page.
	///
	/// The app identity comes from GitHub's authenticated metadata rather than
	/// an assumed public slug. Only a single valid path segment is accepted,
	/// and the resulting link always points to GitHub.
	///
	/// # Errors
	/// Returns an error if app signing, the GitHub request, or slug validation fails.
	pub async fn installation_url(&self, http: &reqwest::Client) -> Result<String, GitHubAppError> {
		let metadata: AppMetadata = self.app_get(http, "/app", "read app metadata").await?;
		let slug = metadata.slug;

		if slug.is_empty()
			|| !slug
				.bytes()
				.all(|character| character.is_ascii_alphanumeric() || character == b'-')
		{
			return Err(GitHubAppError::InvalidAppSlug);
		}

		Ok(format!("https://github.com/apps/{slug}/installations/new"))
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
			.header("User-Agent", "monochange")
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

	/// Look up the account an installation belongs to.
	///
	/// Installations recorded before organisations existed only stored the
	/// account login; this recovers the stable account id once.
	///
	/// # Errors
	/// Returns an error if app signing or the GitHub request fails.
	pub async fn installation_account(
		&self,
		http: &reqwest::Client,
		installation_id: i64,
	) -> Result<monochange_app_db::projects::AccountIdentity, GitHubAppError> {
		let installation: InstallationResponse = self
			.app_get(
				http,
				&format!("/app/installations/{installation_id}"),
				"read installation",
			)
			.await?;
		Ok(monochange_app_db::projects::AccountIdentity {
			provider: monochange_app_db::projects::GITHUB.to_owned(),
			external_id: installation.account.id,
			login: installation.account.login,
			account_type: installation.account.account_type,
			avatar_url: installation.account.avatar_url,
		})
	}

	/// The permissions the app registration currently requests.
	///
	/// # Errors
	/// Returns an error if app signing or the GitHub request fails.
	pub async fn requested_permissions(
		&self,
		http: &reqwest::Client,
	) -> Result<PermissionLevels, GitHubAppError> {
		let metadata: AppMetadata = self.app_get(http, "/app", "read app metadata").await?;
		Ok(metadata.permissions)
	}

	/// The permissions one installation has accepted.
	///
	/// # Errors
	/// Returns an error if app signing or the GitHub request fails.
	pub async fn installation_permissions(
		&self,
		http: &reqwest::Client,
		installation_id: i64,
	) -> Result<PermissionLevels, GitHubAppError> {
		let installation: InstallationResponse = self
			.app_get(
				http,
				&format!("/app/installations/{installation_id}"),
				"read installation",
			)
			.await?;
		Ok(installation.permissions)
	}

	/// Read one app-authenticated GitHub API resource.
	async fn app_get<T: serde::de::DeserializeOwned>(
		&self,
		http: &reqwest::Client,
		path: &str,
		action: &'static str,
	) -> Result<T, GitHubAppError> {
		let response = http
			.get(format!("{}{path}", self.api_url))
			.bearer_auth(self.app_jwt()?)
			.header("Accept", "application/vnd.github+json")
			.header("User-Agent", "monochange")
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status();
			let body = response.text().await.unwrap_or_default();

			return Err(GitHubAppError::Status(action, status, body));
		}

		Ok(response.json().await?)
	}
}

/// GitHub permission names mapped to their access level (`read`, `write`, or
/// `admin`), as the app and installation endpoints report them.
pub type PermissionLevels = BTreeMap<String, String>;

/// A permission the app requests at a higher level than an installation has
/// accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPermission {
	/// GitHub's permission name, such as `actions` or `checks`.
	pub name: String,
	/// The level the app now requests.
	pub requested: String,
	/// The level the installation accepted, or `None` without access.
	pub granted: Option<String>,
}

/// Permissions the app requests that an installation hasn't accepted yet.
///
/// GitHub keeps an installation on its old permissions until an owner of the
/// account approves the app's new request, so any entry here means the
/// installation is waiting for that approval.
#[must_use]
pub fn pending_permissions(
	requested: &PermissionLevels,
	granted: &PermissionLevels,
) -> Vec<PendingPermission> {
	requested
		.iter()
		.filter(|(name, level)| {
			access_rank(level) > granted.get(*name).map_or(0, |level| access_rank(level))
		})
		.map(|(name, level)| {
			PendingPermission {
				name: name.clone(),
				requested: level.clone(),
				granted: granted.get(name).cloned(),
			}
		})
		.collect()
}

fn access_rank(level: &str) -> u8 {
	match level {
		"read" => 1,
		"write" => 2,
		"admin" => 3,
		_ => 0,
	}
}

/// Compare two byte strings without early exit.
///
/// Shared by webhook signature and API token checks so a secret can never be
/// probed byte by byte through either path.
pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
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
struct AppMetadata {
	slug: String,
	#[serde(default)]
	permissions: PermissionLevels,
}

#[derive(Debug, Deserialize)]
struct InstallationResponse {
	account: InstallationAccountResponse,
	#[serde(default)]
	permissions: PermissionLevels,
}

#[derive(Debug, Deserialize)]
struct InstallationAccountResponse {
	id: i64,
	login: String,
	#[serde(rename = "type")]
	account_type: String,
	#[serde(default)]
	avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InstallationTokenResponse {
	token: String,
}

#[derive(Debug, Serialize)]
struct TreeEntry<'a> {
	path: &'a str,
	mode: &'static str,
	#[serde(rename = "type")]
	kind: &'static str,
	#[serde(flatten)]
	source: TreeEntrySource<'a>,
}

/// What a release tree entry writes at its path.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum TreeEntrySource<'a> {
	/// Text GitHub stores as a new blob while creating the tree, so the file
	/// needs no blob request of its own.
	Content { content: &'a str },
	/// Serializes as `"sha": null`, which GitHub requires to delete a path.
	Deleted { sha: () },
}

#[derive(Debug, Serialize)]
struct TreeRequest<'a> {
	base_tree: &'a str,
	tree: Vec<TreeEntry<'a>>,
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
struct GitTreeListing {
	tree: Vec<GitTreeListingEntry>,
	#[serde(default)]
	truncated: bool,
}

#[derive(Debug, Deserialize)]
struct GitTreeListingEntry {
	path: String,
	sha: String,
	#[serde(rename = "type")]
	kind: String,
}

#[derive(Debug, Deserialize)]
struct GitCommitTree {
	tree: TreeResponse,
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

#[derive(Debug, Deserialize)]
struct RepositoryResponse {
	default_branch: String,
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
	node_id: String,
	#[serde(default)]
	body: Option<String>,
	user: PullRequestAuthor,
	head: PullRequestBranch,
	base: PullRequestBranch,
}

#[derive(Debug, Deserialize)]
struct PullRequestAuthor {
	id: u64,
	#[serde(rename = "type")]
	kind: String,
}

#[derive(Debug, Deserialize)]
struct PullRequestBranch {
	#[serde(rename = "ref")]
	name: String,
	repo: PullRequestRepository,
}

#[derive(Debug, Deserialize)]
struct PullRequestRepository {
	id: i64,
}

/// GitHub assigns these identities; callers cannot copy a marker to another PR.
#[derive(Serialize)]
struct ReleasePullRequestIdentity<'a> {
	number: u64,
	node_id: &'a str,
	author_id: u64,
	head_repository: i64,
	head_branch: &'a str,
	base_repository: i64,
	base_branch: &'a str,
}

const RELEASE_PR_MARKER: &str = "<!-- monochange-release-pr:v1:";

impl PullRequestResponse {
	/// Authenticate GitHub-assigned identity separately from caller-controlled prose.
	fn binding(&self, secret: &str) -> Hmac<Sha256> {
		let identity = ReleasePullRequestIdentity {
			number: self.number,
			node_id: &self.node_id,
			author_id: self.user.id,
			head_repository: self.head.repo.id,
			head_branch: &self.head.name,
			base_repository: self.base.repo.id,
			base_branch: &self.base.name,
		};
		let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
			.unwrap_or_else(|error| panic!("HMAC accepts any key length: {error}"));
		mac.update(b"monochange-hosted-release-pr-v1\0");
		mac.update(
			&serde_json::to_vec(&identity)
				.unwrap_or_else(|error| panic!("release PR identity is serializable: {error}")),
		);
		mac
	}

	/// Require the exact repository and branches before trusting a signed marker.
	fn verify_identity(
		&self,
		request: &SourceChangeRequest,
		repository_id: i64,
	) -> Result<(), GitHubAppError> {
		if self.user.kind != "Bot"
			|| self.node_id.is_empty()
			|| self.head.repo.id != repository_id
			|| self.base.repo.id != repository_id
			|| self.head.name != request.head_branch
			|| self.base.name != request.base_branch
		{
			return Err(GitHubAppError::UnboundPullRequest);
		}
		Ok(())
	}

	/// Refuse adoption unless this deployment previously signed the same PR.
	fn verify_binding(&self, secret: &str) -> Result<(), GitHubAppError> {
		let signature = self
			.body
			.as_deref()
			.and_then(|body| body.lines().last())
			.and_then(|line| line.strip_prefix(RELEASE_PR_MARKER))
			.and_then(|line| line.strip_suffix(" -->"))
			.and_then(|signature| hex::decode(signature).ok())
			.ok_or(GitHubAppError::UnboundPullRequest)?;
		self.binding(secret)
			.verify_slice(&signature)
			.map_err(|_| GitHubAppError::UnboundPullRequest)
	}

	/// Require a superseded PR to be a cut PR for the same head and base
	/// series as its replacement, so a refresh cannot close an unrelated
	/// release request that this deployment also signed.
	fn verify_supersedable(
		&self,
		replacement: &Self,
		repository_id: i64,
	) -> Result<(), GitHubAppError> {
		let same_heads = match (
			cut_head_series(&self.head.name),
			cut_head_series(&replacement.head.name),
		) {
			(Some((old_prefix, _)), Some((new_prefix, _))) => old_prefix == new_prefix,
			_ => false,
		};
		let same_bases = self.base.name == replacement.base.name
			|| match (
				cut_base_series(&self.base.name),
				cut_base_series(&replacement.base.name),
			) {
				(Some((old_prefix, _)), Some((new_prefix, _))) => old_prefix == new_prefix,
				_ => false,
			};

		if self.head.repo.id != repository_id
			|| self.base.repo.id != repository_id
			|| !same_heads
			|| !same_bases
		{
			return Err(GitHubAppError::UnboundPullRequest);
		}

		Ok(())
	}

	/// Preserve service provenance when replacing the caller's release notes.
	fn signed_body(&self, secret: &str, body: &str) -> String {
		let signature = hex::encode(self.binding(secret).finalize().into_bytes());
		format!("{body}\n\n{RELEASE_PR_MARKER}{signature} -->")
	}
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
	base: &'a str,
}

#[derive(Debug, Serialize)]
struct UpdatePullRequestBodyRequest<'a> {
	body: &'a str,
}

#[derive(Debug, Serialize)]
struct ClosePullRequestRequest {
	state: &'static str,
}

#[derive(Debug, Serialize)]
struct IssueCommentRequest<'a> {
	body: &'a str,
}

#[derive(Debug, Deserialize)]
struct CompareResponse {
	status: String,
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

/// Read the head commit of a branch, or `None` when the branch does not exist.
async fn read_branch_head(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	request: &HostedCommitRequest,
	branch: &str,
	operation: &'static str,
) -> Result<Option<String>, GitHubAppError> {
	let response = http
		.get(format!(
			"{api_url}/repos/{}/{}/git/ref/heads/{branch}",
			request.owner, request.repository
		))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.send()
		.await?;

	if response.status() == reqwest::StatusCode::NOT_FOUND {
		return Ok(None);
	}

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();

		return Err(api_error(operation, status, body));
	}

	let reference: RefResponse = response.json().await?;
	Ok(Some(reference.object.sha))
}

/// Read the repository's default branch.
async fn read_default_branch(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	owner: &str,
	repo: &str,
) -> Result<String, GitHubAppError> {
	let response = http
		.get(format!("{api_url}/repos/{owner}/{repo}"))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.send()
		.await?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();

		return Err(api_error("read repository", status, body));
	}

	let repository: RepositoryResponse = response.json().await?;
	Ok(repository.default_branch)
}

/// Whether `branch` has the shape of a monochange-owned release or sync branch:
/// - `<branch_prefix>/release` (rolling release PR)
/// - `<branch_prefix>/release/<cut_id>` (frozen cut release PR)
/// - `<branch_prefix>/sync-release` (post-release bookkeeping sync PR)
fn is_release_branch_name(branch: &str) -> bool {
	if let Some((prefix, name)) = branch.rsplit_once('/')
		&& !prefix.is_empty()
		&& matches!(name, "release" | "sync-release")
	{
		return true;
	}

	if let Some((prefix, rest)) = branch.split_once("/release/")
		&& !prefix.is_empty()
		&& !rest.is_empty()
		&& !rest.starts_with('/')
		&& !rest.ends_with('/')
	{
		return true;
	}

	false
}

/// Whether `branch` has the shape of a monochange frozen cut base branch
/// (`release/<cut_id>`, `releases/<cut_id>`, or `<prefix>/release/<cut_id>`).
fn is_cut_base_branch_name(branch: &str) -> bool {
	cut_base_series(branch).is_some()
}

/// Split a frozen cut head (`<prefix>/release/<cut_id>`) into its series
/// prefix and cut id.
fn cut_head_series(branch: &str) -> Option<(&str, &str)> {
	let (prefix, cut_id) = branch.split_once("/release/")?;
	if prefix.is_empty() || cut_id.is_empty() || cut_id.starts_with('/') || cut_id.ends_with('/') {
		return None;
	}
	Some((prefix, cut_id))
}

/// Split a frozen cut base (`release/<cut_id>`, `releases/<cut_id>`, or
/// `<prefix>/release/<cut_id>`) into its series prefix and cut id.
fn cut_base_series(branch: &str) -> Option<(&str, &str)> {
	if let Some(cut_id) = branch
		.strip_prefix("release/")
		.or_else(|| branch.strip_prefix("releases/"))
		&& !cut_id.is_empty()
		&& !cut_id.starts_with('/')
		&& !cut_id.ends_with('/')
	{
		let (prefix, _) = branch.split_once('/')?;
		return Some((prefix, cut_id));
	}

	let (prefix, cut_id) = branch.split_once("/release/")?;
	if prefix.is_empty() || cut_id.is_empty() || cut_id.starts_with('/') || cut_id.ends_with('/') {
		return None;
	}
	Some((prefix, cut_id))
}

/// Verify via GitHub's compare API that `ancestor` is an ancestor of (or identical to) `descendant`.
async fn is_commit_ancestor(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	owner: &str,
	repo: &str,
	ancestor: &str,
	descendant: &str,
) -> Result<bool, GitHubAppError> {
	if ancestor == descendant {
		return Ok(true);
	}

	let response = http
		.get(format!(
			"{api_url}/repos/{owner}/{repo}/compare/{ancestor}...{descendant}"
		))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.send()
		.await?;

	if response.status() == reqwest::StatusCode::NOT_FOUND {
		return Ok(false);
	}

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(api_error("compare base commit ancestry", status, body));
	}

	let compare: CompareResponse = response.json().await?;
	Ok(matches!(compare.status.as_str(), "ahead" | "identical"))
}

/// How the release commit moves the release branch.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ReleaseBranchUpdate {
	/// The release branch does not exist yet.
	Create,
	/// The release branch points at the base commit, so the release commit
	/// fast-forwards it.
	FastForward,
	/// The release branch holds an earlier release commit and is replaced.
	Regenerate,
}

/// How the release commit moves the frozen cut base branch.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum CutBaseUpdate {
	/// The cut base branch does not exist yet.
	Create,
	/// The cut base branch already points at the base commit.
	Unchanged,
	/// The cut base branch is an ancestor of the base commit and advances.
	FastForward,
	/// The cut base branch belongs to the active cut's history and is
	/// replaced, for example after the source branch was rebased.
	Replace,
}

/// The branch mutations one release commit may perform.
#[derive(Debug, Clone, Eq, PartialEq)]
struct ReleaseBranchPlan {
	/// How the release branch moves.
	release: ReleaseBranchUpdate,
	/// The release branch head observed while planning, when it exists.
	release_head: Option<String>,
	/// The cut base mutation for a frozen cut request.
	cut_base: Option<CutBaseUpdate>,
}

/// Whether `base_commit` is the head of the named base branch, or an
/// authorized ancestor when the request opted into ancestor checks.
async fn base_is_authorized(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	request: &HostedCommitRequest,
	base_head: &str,
) -> Result<bool, GitHubAppError> {
	if base_head == request.base_commit {
		return Ok(true);
	}

	if !request.allow_base_ancestor {
		return Ok(false);
	}

	is_commit_ancestor(
		http,
		api_url,
		token,
		&request.owner,
		&request.repository,
		&request.base_commit,
		base_head,
	)
	.await
}

/// Plan the frozen cut base ref mutation.
///
/// A new cut creates the branch. A cut base that is already at `base_commit`
/// is unchanged, and one that is an ancestor fast-forwards without `force`, so
/// GitHub keeps enforcing the fast-forward when the branch moves concurrently.
/// Any other existing tip is replaced only when it is provably part of the
/// active release branch's history; an unrelated branch with the cut base name
/// fails closed instead of being overwritten.
async fn plan_cut_base_update(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	request: &HostedCommitRequest,
	cut_base: &str,
	release_head: Option<&str>,
) -> Result<CutBaseUpdate, GitHubAppError> {
	let Some(existing) = read_branch_head(
		http,
		api_url,
		token,
		request,
		cut_base,
		"read cut base branch ref",
	)
	.await?
	else {
		return Ok(CutBaseUpdate::Create);
	};

	if existing == request.base_commit {
		return Ok(CutBaseUpdate::Unchanged);
	}

	if is_commit_ancestor(
		http,
		api_url,
		token,
		&request.owner,
		&request.repository,
		&existing,
		&request.base_commit,
	)
	.await?
	{
		return Ok(CutBaseUpdate::FastForward);
	}

	let conflict = || {
		GitHubAppError::CutBaseConflict(
			cut_base.to_string(),
			existing.clone(),
			request.base_commit.clone(),
		)
	};

	// A newer cut base must not be rolled back by a stale run.
	if is_commit_ancestor(
		http,
		api_url,
		token,
		&request.owner,
		&request.repository,
		&request.base_commit,
		&existing,
	)
	.await?
	{
		return Err(conflict());
	}

	let Some(release_head) = release_head else {
		return Err(conflict());
	};

	if is_commit_ancestor(
		http,
		api_url,
		token,
		&request.owner,
		&request.repository,
		&existing,
		release_head,
	)
	.await?
	{
		Ok(CutBaseUpdate::Replace)
	} else {
		Err(conflict())
	}
}

/// Decide how the release commit may move the release branch and cut base.
///
/// The release branch belongs to monochange and is rebuilt from the base
/// branch on every run. All updates require a release branch distinct from
/// the named base, when supplied, and the repository's default branch.
/// Replacing an existing release additionally requires a named base branch
/// whose head is still `base_commit`, or—when `request.allow_base_ancestor`
/// is set for frozen cuts and sync commits—whose history contains `base_commit`.
///
/// A frozen cut must name the normal branch it was prepared from, so its
/// `base_commit` can be checked against that branch's history. Its cut base
/// branch may only fast-forward, or be replaced when the existing tip is
/// provably part of the active cut's history (see [`plan_cut_base_update`]).
///
/// A stale run whose base branch moved gets [`GitHubAppError::BaseMoved`] so
/// the run for the newer commit refreshes the branch. Invalid branch policy
/// gets [`GitHubAppError::InvalidReleaseBranch`]. An unrelated cut base tip
/// gets [`GitHubAppError::CutBaseConflict`]. Without these checks a caller
/// could name `main` as the branch and force it onto another branch's commit.
async fn plan_release_branch_update(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	request: &HostedCommitRequest,
) -> Result<ReleaseBranchPlan, GitHubAppError> {
	if !is_release_branch_name(&request.branch)
		|| request.base_branch.as_deref() == Some(request.branch.as_str())
	{
		return Err(GitHubAppError::InvalidReleaseBranch(request.branch.clone()));
	}

	let default_branch =
		read_default_branch(http, api_url, token, &request.owner, &request.repository).await?;

	if default_branch == request.branch {
		return Err(GitHubAppError::InvalidReleaseBranch(request.branch.clone()));
	}

	let cut_source = if let Some(cut_base) = request.cut_base_branch.as_deref() {
		if !is_cut_base_branch_name(cut_base)
			|| cut_base == default_branch
			|| cut_base == request.branch
			|| request.base_branch.as_deref() == Some(cut_base)
		{
			return Err(GitHubAppError::InvalidReleaseBranch(cut_base.to_string()));
		}

		let base_branch = request
			.base_branch
			.as_deref()
			.ok_or_else(|| GitHubAppError::CutBaseRequiresBase(cut_base.to_string()))?;

		if is_release_branch_name(base_branch) || is_cut_base_branch_name(base_branch) {
			return Err(GitHubAppError::InvalidCutBase(
				cut_base.to_string(),
				base_branch.to_string(),
			));
		}

		Some(base_branch)
	} else {
		None
	};

	let release_head = read_branch_head(
		http,
		api_url,
		token,
		request,
		&request.branch,
		"read release branch ref",
	)
	.await?;

	let cut_base = if let (Some(cut_base), Some(base_branch)) =
		(request.cut_base_branch.as_deref(), cut_source)
	{
		let base_head = read_branch_head(
			http,
			api_url,
			token,
			request,
			base_branch,
			"read base branch ref",
		)
		.await?
		.ok_or_else(|| {
			api_error(
				"read base branch ref",
				reqwest::StatusCode::NOT_FOUND,
				format!("base branch `{base_branch}` does not exist"),
			)
		})?;

		if !base_is_authorized(http, api_url, token, request, &base_head).await? {
			return Err(GitHubAppError::BaseMoved(
				base_branch.to_string(),
				base_head,
				request.base_commit.clone(),
			));
		}

		Some(
			plan_cut_base_update(
				http,
				api_url,
				token,
				request,
				cut_base,
				release_head.as_deref(),
			)
			.await?,
		)
	} else {
		None
	};

	let release = match release_head.as_deref() {
		None => ReleaseBranchUpdate::Create,
		Some(head) if head == request.base_commit => ReleaseBranchUpdate::FastForward,
		Some(head) => {
			let base_branch = request.base_branch.as_deref().ok_or_else(|| {
				GitHubAppError::BranchMoved(
					request.branch.clone(),
					head.to_string(),
					request.base_commit.clone(),
				)
			})?;

			// A cut already validated the base branch above.
			if cut_source.is_none() {
				let base_head = read_branch_head(
					http,
					api_url,
					token,
					request,
					base_branch,
					"read base branch ref",
				)
				.await?
				.ok_or_else(|| {
					api_error(
						"read base branch ref",
						reqwest::StatusCode::NOT_FOUND,
						format!("base branch `{base_branch}` does not exist"),
					)
				})?;

				if !base_is_authorized(http, api_url, token, request, &base_head).await? {
					return Err(GitHubAppError::BaseMoved(
						base_branch.to_string(),
						base_head,
						request.base_commit.clone(),
					));
				}
			}

			ReleaseBranchUpdate::Regenerate
		}
	};

	Ok(ReleaseBranchPlan {
		release,
		release_head,
		cut_base,
	})
}

/// Inline file bytes sent in one tree request before the rest of the release
/// is layered on in a further tree request chained through `base_tree`.
const RELEASE_TREE_INLINE_BYTES_PER_REQUEST: usize = 4 * 1024 * 1024;

/// Build a tree holding the request's files on top of the prepared base
/// commit.
///
/// File contents travel inline in the tree request, so GitHub creates the
/// blobs itself. One blob request per file would trip GitHub's secondary
/// rate limit for content creation on releases touching more than about 80
/// files. Releases with more than `inline_bytes` of content are split across
/// tree requests that each build on the previous tree.
async fn create_release_tree(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	request: &HostedCommitRequest,
	inline_bytes: usize,
) -> Result<String, GitHubAppError> {
	let owner = &request.owner;
	let repo = &request.repository;
	let mut tree = request.base_commit.clone();

	for chunk in release_tree_chunks(&request.files, inline_bytes) {
		let response = http
			.post(format!("{api_url}/repos/{owner}/{repo}/git/trees"))
			.bearer_auth(installation_token)
			.header("Accept", "application/vnd.github+json")
			.json(&TreeRequest {
				base_tree: &tree,
				tree: chunk,
			})
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status();
			let body = response.text().await.unwrap_or_default();

			return Err(api_error("create tree", status, body));
		}

		let tree_response: TreeResponse = response.json().await?;
		tree = tree_response.sha;
	}

	Ok(tree)
}

/// Split the release files into tree requests carrying at most
/// `inline_bytes` of content each. There is always at least one request, so
/// the result is a tree even when the request lists no files.
fn release_tree_chunks(files: &[HostedCommitFile], inline_bytes: usize) -> Vec<Vec<TreeEntry<'_>>> {
	let mut chunks = Vec::new();
	let mut chunk = Vec::new();
	let mut chunk_bytes = 0;

	for file in files {
		let source = match &file.content {
			Some(content) => TreeEntrySource::Content { content },
			None => TreeEntrySource::Deleted { sha: () },
		};

		let entry_bytes = file.path.len() + file.content.as_ref().map_or(0, String::len);

		if !chunk.is_empty() && chunk_bytes + entry_bytes > inline_bytes {
			chunks.push(std::mem::take(&mut chunk));
			chunk_bytes = 0;
		}

		chunk_bytes += entry_bytes;
		chunk.push(TreeEntry {
			path: &file.path,
			mode: "100644",
			kind: "blob",
			source,
		});
	}

	chunks.push(chunk);
	chunks
}

/// GET a Git Database object from the repository.
async fn read_git_object<T: serde::de::DeserializeOwned>(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	request: &HostedCommitRequest,
	object: &str,
	operation: &'static str,
) -> Result<T, GitHubAppError> {
	let response = http
		.get(format!(
			"{api_url}/repos/{}/{}/git/{object}",
			request.owner, request.repository
		))
		.bearer_auth(token)
		.header("Accept", "application/vnd.github+json")
		.send()
		.await?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();

		return Err(api_error(operation, status, body));
	}

	Ok(response.json().await?)
}

/// The object id of the entry `name` directly inside `tree`, if any.
async fn tree_child(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	request: &HostedCommitRequest,
	tree: Option<&str>,
	name: &str,
) -> Result<Option<String>, GitHubAppError> {
	let Some(tree) = tree else {
		return Ok(None);
	};
	let listing: GitTreeListing = read_git_object(
		http,
		api_url,
		token,
		request,
		&format!("trees/{tree}"),
		"read release tree",
	)
	.await?;

	if listing.truncated {
		return Err(GitHubAppError::WorkflowsChanged);
	}

	let entry = listing.tree.into_iter().find(|entry| entry.path == name);

	if entry.as_ref().is_some_and(|entry| entry.kind != "tree") {
		return Err(GitHubAppError::WorkflowsChanged);
	}

	Ok(entry.map(|entry| entry.sha))
}

/// Refuse a release tree when it changes `.github/workflows`.
///
/// Both uploaded files and caller-built trees must preserve the base commit's
/// workflow subtree, including its object types.
async fn ensure_tree_keeps_workflows(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	request: &HostedCommitRequest,
	tree: &str,
) -> Result<(), GitHubAppError> {
	let base: GitCommitTree = read_git_object(
		http,
		api_url,
		token,
		request,
		&format!("commits/{}", request.base_commit),
		"read base commit",
	)
	.await?;
	let mut base_entry = Some(base.tree.sha);
	let mut release_entry = Some(tree.to_string());

	for name in [".github", "workflows"] {
		if base_entry == release_entry {
			return Ok(());
		}

		base_entry = tree_child(http, api_url, token, request, base_entry.as_deref(), name).await?;
		release_entry = tree_child(
			http,
			api_url,
			token,
			request,
			release_entry.as_deref(),
			name,
		)
		.await?;
	}

	if base_entry == release_entry {
		Ok(())
	} else {
		Err(GitHubAppError::WorkflowsChanged)
	}
}

/// Apply the planned frozen cut base ref mutation.
///
/// Fast-forwards keep GitHub's own non-forced update check; only a verified
/// same-cut replacement uses `force`.
async fn apply_cut_base_update(
	http: &reqwest::Client,
	api_url: &str,
	token: &str,
	request: &HostedCommitRequest,
	update: Option<CutBaseUpdate>,
) -> Result<(), GitHubAppError> {
	let (Some(cut_base), Some(update)) = (request.cut_base_branch.as_deref(), update) else {
		return Ok(());
	};

	let response = match update {
		CutBaseUpdate::Unchanged => return Ok(()),
		CutBaseUpdate::Create => {
			http.post(format!(
				"{api_url}/repos/{}/{}/git/refs",
				request.owner, request.repository
			))
			.bearer_auth(token)
			.header("Accept", "application/vnd.github+json")
			.json(&CreateRefRequest {
				reference: format!("refs/heads/{cut_base}"),
				sha: &request.base_commit,
			})
			.send()
			.await?
		}
		CutBaseUpdate::FastForward | CutBaseUpdate::Replace => {
			http.patch(format!(
				"{api_url}/repos/{}/{}/git/refs/heads/{cut_base}",
				request.owner, request.repository
			))
			.bearer_auth(token)
			.header("Accept", "application/vnd.github+json")
			.json(&UpdateRefRequest {
				sha: &request.base_commit,
				force: update == CutBaseUpdate::Replace,
			})
			.send()
			.await?
		}
	};

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		let operation = if update == CutBaseUpdate::Create {
			"create cut base branch ref"
		} else {
			"update cut base branch ref"
		};

		return Err(api_error(operation, status, body));
	}

	Ok(())
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

	// 1. Decide how the release branch and cut base move before writing anything.
	let plan = plan_release_branch_update(http, api_url, installation_token, request).await?;

	// 2. The tree: either the one the workflow already built with its own
	// token, or one created here from the uploaded contents.
	let tree_sha = match &request.tree {
		Some(tree) => tree.clone(),
		None => {
			create_release_tree(
				http,
				api_url,
				installation_token,
				request,
				RELEASE_TREE_INLINE_BYTES_PER_REQUEST,
			)
			.await?
		}
	};

	ensure_tree_keeps_workflows(http, api_url, installation_token, request, &tree_sha).await?;

	// 4. Commit with the prepared base as first parent plus any bridge parents.
	let message = if request.body.is_empty() {
		request.subject.clone()
	} else {
		format!("{}\n\n{}", request.subject, request.body)
	};
	let mut parents = Vec::with_capacity(1 + request.additional_parents.len());
	parents.push(base_commit.as_str());

	for parent in &request.additional_parents {
		parents.push(parent.as_str());
	}

	let response = http
		.post(format!("{api_url}/repos/{owner}/{repo}/git/commits"))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.json(&CommitRequest {
			message,
			tree: &tree_sha,
			parents,
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
		return Err(GitHubAppError::UnverifiedCommit(
			commit.sha,
			verification_reason.unwrap_or_else(|| "unknown".to_string()),
		));
	}

	// 4b. Decide whether the release update must be forced, then move the cut
	// base branch (`release/<cut_id>`) to `base_commit` before moving the
	// release head.
	let update = plan.release;
	let force = match (update, plan.release_head.as_deref()) {
		(ReleaseBranchUpdate::Create | ReleaseBranchUpdate::FastForward, _) => false,
		(ReleaseBranchUpdate::Regenerate, Some(release_head)) => {
			!is_commit_ancestor(
				http,
				api_url,
				installation_token,
				owner,
				repo,
				release_head,
				&commit.sha,
			)
			.await?
		}
		// Unreachable: a regenerate is only planned when the head exists.
		(ReleaseBranchUpdate::Regenerate, None) => true,
	};

	apply_cut_base_update(http, api_url, installation_token, request, plan.cut_base).await?;

	// 5. Point the release branch at the new commit. By this point the commit
	// is always verified, since an unverified commit returns `UnverifiedCommit`
	// before the ref is updated here. A regenerate is forced only when the new
	// commit does not already contain the current release head; bridge commits
	// with integrated parents stay protected by GitHub's fast-forward check.
	let operation = match update {
		ReleaseBranchUpdate::Create => "create release branch ref",
		ReleaseBranchUpdate::FastForward | ReleaseBranchUpdate::Regenerate => {
			"update release branch ref"
		}
	};

	let response = match update {
		ReleaseBranchUpdate::Create => {
			http.post(format!("{api_url}/repos/{owner}/{repo}/git/refs"))
				.json(&CreateRefRequest {
					reference: format!("refs/heads/{}", request.branch),
					sha: &commit.sha,
				})
		}
		ReleaseBranchUpdate::FastForward | ReleaseBranchUpdate::Regenerate => {
			http.patch(format!(
				"{api_url}/repos/{owner}/{repo}/git/refs/heads/{}",
				request.branch
			))
			.json(&UpdateRefRequest {
				sha: &commit.sha,
				force,
			})
		}
	}
	.bearer_auth(installation_token)
	.header("Accept", "application/vnd.github+json")
	.send()
	.await?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();

		return Err(api_error(operation, status, body));
	}

	Ok((commit.sha, verified, verification_reason))
}

/// Extra options controlling hosted cut PR banner updates and PR superseding.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct PublishPullRequestOptions {
	/// Only update the body of an existing verified release PR without creating
	/// a new pull request or mutating labels/auto-merge.
	pub banner_only: bool,
	/// Optional older release PR number to comment on and close after creating a
	/// superseding cut PR in the same head and base series (verified via HMAC
	/// binding before closing).
	pub supersede_pr_number: Option<u64>,
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
	let head = format!("{owner}:{head_branch}");
	let response = http
		.get(format!("{api_url}/repos/{owner}/{repo}/pulls"))
		.query(&[("state", "open"), ("head", head.as_str())])
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

	if pulls.len() > 1 {
		return Err(GitHubAppError::UnboundPullRequest);
	}

	Ok(pulls.into_iter().next())
}

/// Open or update the release pull request under the bot identity.
///
/// Existing PRs must carry a server-authenticated marker for their immutable
/// identity, repository, creator, head and base. A newly created PR receives
/// that marker before labels or auto-merge are applied. Rotating the signing
/// secret invalidates old markers and requires recreating those release PRs.
///
/// Returns the pull request number, the performed operation, and its URL.
pub async fn publish_release_pull_request(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	request: &SourceChangeRequest,
	binding_secret: &str,
	repository_id: i64,
) -> Result<(u64, SourceChangeRequestOperation, String), GitHubAppError> {
	publish_release_pull_request_with_options(
		http,
		api_url,
		installation_token,
		request,
		binding_secret,
		repository_id,
		PublishPullRequestOptions::default(),
	)
	.await
}

/// Open or update a release, cut, or sync pull request with optional banner-only
/// and PR-superseding behavior.
pub async fn publish_release_pull_request_with_options(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	request: &SourceChangeRequest,
	binding_secret: &str,
	repository_id: i64,
	options: PublishPullRequestOptions,
) -> Result<(u64, SourceChangeRequestOperation, String), GitHubAppError> {
	let owner = &request.owner;
	let repo = &request.repo;

	if !is_release_branch_name(&request.head_branch)
		|| request.head_branch == request.base_branch
		|| read_default_branch(http, api_url, installation_token, owner, repo).await?
			== request.head_branch
	{
		return Err(GitHubAppError::InvalidReleaseBranch(
			request.head_branch.clone(),
		));
	}

	let existing = find_open_pull_request(
		http,
		api_url,
		installation_token,
		owner,
		repo,
		&request.head_branch,
	)
	.await?;

	if options.banner_only {
		let Some(existing) = existing else {
			return Err(api_error(
				"update pull request status banner",
				reqwest::StatusCode::NOT_FOUND,
				format!(
					"no open pull request found for head branch `{}`",
					request.head_branch
				),
			));
		};
		existing.verify_identity(request, repository_id)?;
		existing.verify_binding(binding_secret)?;
		let body = existing.signed_body(binding_secret, &request.body);
		let response = http
			.patch(format!(
				"{api_url}/repos/{owner}/{repo}/pulls/{}",
				existing.number
			))
			.bearer_auth(installation_token)
			.header("Accept", "application/vnd.github+json")
			.json(&UpdatePullRequestBodyRequest { body: &body })
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status();
			return Err(api_error(
				"update pull request status banner",
				status,
				response.text().await.unwrap_or_default(),
			));
		}

		let updated: PullRequestResponse = response.json().await?;
		updated.verify_identity(request, repository_id)?;
		updated.verify_binding(binding_secret)?;
		return Ok((
			existing.number,
			SourceChangeRequestOperation::Updated,
			existing.html_url,
		));
	}

	let (pull, operation) = if let Some(existing) = existing {
		existing.verify_identity(request, repository_id)?;
		existing.verify_binding(binding_secret)?;
		(existing, SourceChangeRequestOperation::Updated)
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

	pull.verify_identity(request, repository_id)?;
	let body = pull.signed_body(binding_secret, &request.body);
	let stamp = async {
		let response = http
			.patch(format!(
				"{api_url}/repos/{owner}/{repo}/pulls/{}",
				pull.number
			))
			.bearer_auth(installation_token)
			.header("Accept", "application/vnd.github+json")
			.json(&UpdatePullRequestRequest {
				title: &request.title,
				body: &body,
				base: &request.base_branch,
			})
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status();
			return Err(api_error(
				"update pull request",
				status,
				response.text().await.unwrap_or_default(),
			));
		}

		let updated: PullRequestResponse = response.json().await?;
		updated.verify_identity(request, repository_id)?;
		updated.verify_binding(binding_secret)
	}
	.await;
	stamp.map_err(|error| {
		if operation == SourceChangeRequestOperation::Created {
			GitHubAppError::ReleasePullRequestNeedsRecovery {
				url: pull.html_url.clone(),
				source: Box::new(error),
			}
		} else {
			error
		}
	})?;

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

	if request.auto_merge {
		let _ =
			enable_pull_request_auto_merge(http, api_url, installation_token, &pull.node_id).await;
	}

	if let Some(old_number) = options.supersede_pr_number
		&& old_number != pull.number
	{
		supersede_verified_pull_request(
			http,
			api_url,
			installation_token,
			owner,
			repo,
			old_number,
			&pull,
			binding_secret,
			repository_id,
		)
		.await?;
	}

	Ok((pull.number, operation, pull.html_url))
}

/// Comment on and close a superseded cut PR after confirming its HMAC binding
/// and that it belongs to the same cut series as the replacement.
#[allow(clippy::too_many_arguments)]
async fn supersede_verified_pull_request(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	owner: &str,
	repo: &str,
	old_number: u64,
	replacement: &PullRequestResponse,
	binding_secret: &str,
	repository_id: i64,
) -> Result<(), GitHubAppError> {
	let response = http
		.get(format!("{api_url}/repos/{owner}/{repo}/pulls/{old_number}"))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.send()
		.await?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		return Err(api_error("read superseded pull request", status, body));
	}

	let old_pull: PullRequestResponse = response.json().await?;
	old_pull.verify_binding(binding_secret)?;
	old_pull.verify_supersedable(replacement, repository_id)?;

	let comment = format!(
		"Superseded by #{} ({}) after `/refresh`.",
		replacement.number, replacement.html_url
	);
	let comment_response = http
		.post(format!(
			"{api_url}/repos/{owner}/{repo}/issues/{old_number}/comments"
		))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.json(&IssueCommentRequest { body: &comment })
		.send()
		.await?;

	if !comment_response.status().is_success() {
		let status = comment_response.status();
		let body = comment_response.text().await.unwrap_or_default();
		return Err(api_error(
			"comment on superseded pull request",
			status,
			body,
		));
	}

	let close_response = http
		.patch(format!("{api_url}/repos/{owner}/{repo}/pulls/{old_number}"))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.json(&ClosePullRequestRequest { state: "closed" })
		.send()
		.await?;

	if !close_response.status().is_success() {
		let status = close_response.status();
		let body = close_response.text().await.unwrap_or_default();
		return Err(api_error("close superseded pull request", status, body));
	}

	Ok(())
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

/// An issue the monochange GitHub App opened.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CreatedIssue {
	pub number: u64,
	pub html_url: String,
}

#[derive(Debug, Serialize)]
struct CreateIssueRequest<'a> {
	title: &'a str,
	body: &'a str,
	labels: &'a [String],
}

/// Open an issue in `full_name` (`owner/name`) as the monochange bot.
///
/// # Errors
/// Returns an error if the request fails or GitHub rejects it, for example
/// when the app lacks Issues write access on the repository.
pub async fn create_issue(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	full_name: &str,
	title: &str,
	body: &str,
	labels: &[String],
) -> Result<CreatedIssue, GitHubAppError> {
	let response = http
		.post(format!(
			"{}/repos/{full_name}/issues",
			api_url.trim_end_matches('/')
		))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.json(&CreateIssueRequest {
			title,
			body,
			labels,
		})
		.send()
		.await?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();

		return Err(api_error("create issue", status, body));
	}

	Ok(response.json().await?)
}

/// Whether `path` exists on the default branch of `full_name`.
///
/// # Errors
/// Returns an error if the request fails or GitHub answers with anything but
/// found or not found, for example when the app can't read contents.
pub async fn repository_file_exists(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	full_name: &str,
	path: &str,
) -> Result<bool, GitHubAppError> {
	let response = http
		.get(format!(
			"{}/repos/{full_name}/contents/{path}",
			api_url.trim_end_matches('/')
		))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.send()
		.await?;

	match response.status() {
		reqwest::StatusCode::OK => Ok(true),
		reqwest::StatusCode::NOT_FOUND => Ok(false),
		status => {
			let body = response.text().await.unwrap_or_default();
			Err(api_error("read repository file", status, body))
		}
	}
}

#[derive(Debug, Serialize)]
struct RepositoryDispatchRequest<'a, P: Serialize> {
	event_type: &'a str,
	client_payload: &'a P,
}

/// Send a `repository_dispatch` event named `event_type` to `full_name`.
/// GitHub runs every workflow on the default branch that listens for it.
///
/// # Errors
/// Returns an error if the request fails or GitHub rejects it, for example
/// when the app lacks Contents write access on the repository.
pub async fn dispatch_repository_event<P: Serialize>(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	full_name: &str,
	event_type: &str,
	client_payload: &P,
) -> Result<(), GitHubAppError> {
	let response = http
		.post(format!(
			"{}/repos/{full_name}/dispatches",
			api_url.trim_end_matches('/')
		))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.json(&RepositoryDispatchRequest {
			event_type,
			client_payload,
		})
		.send()
		.await?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();

		return Err(api_error("dispatch repository event", status, body));
	}

	Ok(())
}

#[derive(Debug, Serialize)]
struct WorkflowDispatchRequest<'a> {
	#[serde(rename = "ref")]
	git_ref: &'a str,
	#[serde(skip_serializing_if = "BTreeMap::is_empty")]
	inputs: &'a BTreeMap<String, String>,
}

/// Dispatch a GitHub Actions workflow in `full_name`.
///
/// The caller has already validated the workflow file name or id, the ref, and
/// the inputs. GitHub runs the workflow from a ref that must contain the
/// workflow file.
///
/// # Errors
/// Returns an error if the request fails or GitHub rejects it, for example
/// when the installation lacks Actions write access or the ref does not exist.
pub async fn dispatch_workflow(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	full_name: &str,
	workflow: &str,
	git_ref: &str,
	inputs: &BTreeMap<String, String>,
) -> Result<(), GitHubAppError> {
	let response = http
		.post(format!(
			"{}/repos/{full_name}/actions/workflows/{workflow}/dispatches",
			api_url.trim_end_matches('/')
		))
		.bearer_auth(installation_token)
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "monochange")
		.json(&WorkflowDispatchRequest { git_ref, inputs })
		.send()
		.await?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();

		return Err(api_error("dispatch workflow", status, body));
	}

	Ok(())
}

/// The commit at the tip of `full_name`'s default branch, as a full SHA.
///
/// # Errors
/// Returns an error if either request fails or GitHub answers with anything
/// but the repository and its branch.
pub async fn default_branch_head(
	http: &reqwest::Client,
	api_url: &str,
	installation_token: &str,
	full_name: &str,
) -> Result<String, GitHubAppError> {
	let api_url = api_url.trim_end_matches('/');
	let read = |url: String| {
		http.get(url)
			.bearer_auth(installation_token)
			.header("Accept", "application/vnd.github+json")
			.header("User-Agent", "monochange")
			.send()
	};

	let response = read(format!("{api_url}/repos/{full_name}")).await?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();

		return Err(api_error("read repository", status, body));
	}

	let repository: RepositoryResponse = response.json().await?;
	let response = read(format!(
		"{api_url}/repos/{full_name}/git/ref/heads/{}",
		repository.default_branch
	))
	.await?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();

		return Err(api_error("read default branch", status, body));
	}

	let reference: RefResponse = response.json().await?;
	Ok(reference.object.sha)
}
