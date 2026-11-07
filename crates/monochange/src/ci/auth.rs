//! Repo-scoped authentication and the protected memory store.
//!
//! A token is an HMAC over the canonical repository identity, so a token minted
//! for one repository cannot read or write another repository's memory file
//! even when both live on the same runner. The canonical identity is derived
//! from the `origin` remote so CI checkouts agree with developer machines.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use ring::digest;
use ring::hmac;
use ring::rand::SecureRandom;
use ring::rand::SystemRandom;

use super::CI_AUTH_RELATIVE_PATH;
use super::CI_MEMORY_RELATIVE_PATH;
use super::CiAuthFile;
use super::CiAuthReport;
use super::CiMemoryStore;

/// Number of hex characters kept from the SHA-256 repository digest.
const REPO_HASH_PREFIX_LEN: usize = 16;
/// Entropy of a generated repository secret in bytes.
const SECRET_BYTES: usize = 32;
/// File mode applied to the auth file so the secret is not world readable.
#[cfg(unix)]
const AUTH_FILE_MODE: u32 = 0o600;

/// Prefix that marks a token as a monochange CI token.
const TOKEN_PREFIX: &str = "mc_ci_";
/// Radix used when decoding a hex signature.
const HEX_RADIX: u32 = 16;
/// Domain separator mixed into the token HMAC payload.
const TOKEN_PAYLOAD_PREFIX: &str = "monochange-ci-auth:v1";
/// Auth file schema version written by `ci auth init`.
pub(super) const AUTH_SCHEMA_VERSION: u32 = 1;

/// Normalize a git remote URL or identifier into a canonical repository identity.
///
/// Handles `git@host:owner/repo.git`, `https://`, `http://`, `ssh://`, and
/// `git://` remotes, ignoring credentials and trailing `.git` suffixes.
pub fn normalize_repo_identity(raw: &str) -> String {
	let trimmed = raw.trim().trim_end_matches('/');

	if let Some(identity) = normalize_scp_style_remote(trimmed) {
		return identity;
	}

	if let Some(identity) = normalize_scheme_remote(trimmed) {
		return identity;
	}

	strip_git_suffix(trimmed)
		.unwrap_or(trimmed)
		.to_ascii_lowercase()
}

/// Normalize `git@host:owner/repo.git` style scp remotes.
fn normalize_scp_style_remote(remote: &str) -> Option<String> {
	let rest = remote.strip_prefix("git@")?;
	let (host, path) = rest.split_once(':')?;

	Some(format!(
		"{}/{}",
		host.to_ascii_lowercase(),
		clean_repo_path(path)
	))
}

/// Normalize `https://`, `http://`, `ssh://`, and `git://` remotes.
fn normalize_scheme_remote(remote: &str) -> Option<String> {
	for scheme in ["https://", "http://", "ssh://", "git://"] {
		let Some(rest) = remote.strip_prefix(scheme) else {
			continue;
		};
		let without_user = rest.split_once('@').map_or(rest, |(_, after_at)| after_at);

		if let Some((host, path)) = without_user.split_once('/') {
			return Some(format!(
				"{}/{}",
				host.to_ascii_lowercase(),
				clean_repo_path(path)
			));
		}
	}

	None
}

/// Drop leading slashes and a trailing `.git`, then lowercase the path.
fn clean_repo_path(path: &str) -> String {
	let trimmed = path.trim_start_matches('/');
	let clean = strip_git_suffix(trimmed).unwrap_or(path);

	clean.to_ascii_lowercase()
}

/// Remove a trailing `.git` suffix when present.
fn strip_git_suffix(value: &str) -> Option<&str> {
	value.strip_suffix(".git")
}

/// Derive the canonical repository identity (`repo_id`) for `root`.
///
/// Resolution order: explicit `--repo-id`, `MONOCHANGE_CI_REPO_ID`, the
/// normalized `origin` remote, and finally a `local:<hash>` fallback so
/// untracked scratch directories still get a stable identity.
pub fn derive_canonical_repo_id(root: &Path, repo_id_override: Option<&str>) -> String {
	if let Some(identity) = trimmed_non_empty(repo_id_override) {
		return normalize_repo_identity(identity);
	}

	if let Some(identity) = env_repo_identity() {
		return identity;
	}

	if let Some(origin_url) = resolve_origin_url(root) {
		return normalize_repo_identity(&origin_url);
	}

	fallback_local_repo_id(root)
}

/// Return a trimmed value when it is not blank.
fn trimmed_non_empty(value: Option<&str>) -> Option<&str> {
	value.map(str::trim).filter(|trimmed| !trimmed.is_empty())
}

/// Read and normalize `MONOCHANGE_CI_REPO_ID`.
fn env_repo_identity() -> Option<String> {
	let value = std::env::var("MONOCHANGE_CI_REPO_ID").ok()?;
	let trimmed = value.trim();

	if trimmed.is_empty() {
		return None;
	}

	Some(normalize_repo_identity(trimmed))
}

/// Resolve the `origin` URL from `.git/config`, then from the git CLI.
fn resolve_origin_url(root: &Path) -> Option<String> {
	read_origin_url_from_git_config(root).or_else(|| origin_url_from_git_command(root))
}

/// Ask git for the `origin` URL as a fallback for unusual checkout layouts.
fn origin_url_from_git_command(root: &Path) -> Option<String> {
	let output = ProcessCommand::new("git")
		.current_dir(root)
		.args(["config", "--get", "remote.origin.url"])
		.output()
		.ok()
		.filter(|output| output.status.success())?;
	let stdout = String::from_utf8(output.stdout).ok()?;
	let trimmed = stdout.trim();

	if trimmed.is_empty() {
		return None;
	}

	Some(trimmed.to_string())
}

/// Hash the canonicalized workspace root when no remote identity is available.
fn fallback_local_repo_id(root: &Path) -> String {
	let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
	let digest_bytes = digest::digest(&digest::SHA256, canonical.to_string_lossy().as_bytes());
	let hash_hex = hex_encode(digest_bytes.as_ref());
	let prefix = hash_hex.get(..REPO_HASH_PREFIX_LEN).unwrap_or(&hash_hex);

	format!("local:{prefix}")
}

/// Read the `origin` URL directly from the git config file.
fn read_origin_url_from_git_config(root: &Path) -> Option<String> {
	let config_path = resolve_git_config_path(root)?;
	let config_text = fs::read_to_string(config_path).ok()?;

	origin_url_from_config_text(&config_text)
}

/// Locate the git config, following the `gitdir:` pointer used by worktrees.
fn resolve_git_config_path(root: &Path) -> Option<PathBuf> {
	let dot_git = root.join(".git");

	if dot_git.is_dir() {
		return Some(dot_git.join("config"));
	}

	if !dot_git.is_file() {
		return None;
	}

	resolve_worktree_git_config(root, &dot_git)
}

/// Resolve a worktree `.git` file to the shared config file.
fn resolve_worktree_git_config(root: &Path, dot_git: &Path) -> Option<PathBuf> {
	let content = fs::read_to_string(dot_git).ok()?;
	let gitdir_raw = content
		.lines()
		.find_map(|line| line.strip_prefix("gitdir:"))?
		.trim();
	let gitdir_path = Path::new(gitdir_raw);
	let resolved = if gitdir_path.is_absolute() {
		gitdir_path.to_path_buf()
	} else {
		root.join(gitdir_path)
	};

	if resolved.join("config").is_file() {
		return Some(resolved.join("config"));
	}

	let common_config = resolved.parent()?.parent()?.join("config");

	if common_config.is_file() {
		return Some(common_config);
	}

	None
}

/// Extract the `url` entry from the `[remote "origin"]` config section.
fn origin_url_from_config_text(config_text: &str) -> Option<String> {
	let mut in_origin_section = false;

	for line in config_text.lines() {
		let trimmed = line.trim();
		let is_section_header = trimmed.starts_with('[') && trimmed.ends_with(']');

		if is_section_header {
			in_origin_section = trimmed.eq_ignore_ascii_case("[remote \"origin\"]");
			continue;
		}

		if let Some(url) = origin_url_from_config_line(trimmed, in_origin_section) {
			return Some(url);
		}
	}

	None
}

/// Parse one `key = value` config line, returning the origin URL when it matches.
fn origin_url_from_config_line(line: &str, in_origin_section: bool) -> Option<String> {
	if !in_origin_section {
		return None;
	}

	let (key, value) = line.split_once('=')?;

	if !key.trim().eq_ignore_ascii_case("url") {
		return None;
	}

	let url = value.trim().trim_matches('"');

	if url.is_empty() {
		return None;
	}

	Some(url.to_string())
}

/// Compute the 16-char hex hash of the canonical `repo_id`.
pub fn compute_repo_hash(repo_id: &str) -> String {
	let normalized = normalize_repo_identity(repo_id);
	let digest_bytes = digest::digest(&digest::SHA256, normalized.as_bytes());
	let full = hex_encode(digest_bytes.as_ref());

	full.get(..REPO_HASH_PREFIX_LEN)
		.unwrap_or(&full)
		.to_string()
}

/// Issue a repo-scoped HMAC-SHA256 token `mc_ci_<repo_hash>_<hmac>`.
pub fn issue_repo_token(repo_id: &str, secret: &str) -> String {
	let normalized = normalize_repo_identity(repo_id);
	let repo_hash = compute_repo_hash(&normalized);
	let key = hmac::Key::new(hmac::HMAC_SHA256, secret.trim().as_bytes());
	let payload = format!("{TOKEN_PAYLOAD_PREFIX}:{normalized}:{repo_hash}");
	let tag = hmac::sign(&key, payload.as_bytes());
	let hmac_hex = hex_encode(tag.as_ref());

	format!("{TOKEN_PREFIX}{repo_hash}_{hmac_hex}")
}

/// Verify that `token` is structurally valid and matches `expected_repo_id` and `secret`.
pub fn verify_repo_token(
	token: &str,
	expected_repo_id: &str,
	secret: &str,
) -> MonochangeResult<()> {
	let (token_repo_hash, token_hmac_hex) = parse_repo_token(token)?;
	let normalized_repo_id = normalize_repo_identity(expected_repo_id);
	let expected_repo_hash = compute_repo_hash(&normalized_repo_id);

	if token_repo_hash != expected_repo_hash {
		return Err(MonochangeError::Config(format!(
			"CI auth token repository mismatch: token belongs to repo hash `{token_repo_hash}`, but current repository `{normalized_repo_id}` has hash `{expected_repo_hash}` (cross-repo access denied)"
		)));
	}

	let key = hmac::Key::new(hmac::HMAC_SHA256, secret.trim().as_bytes());
	let payload = format!("{TOKEN_PAYLOAD_PREFIX}:{normalized_repo_id}:{expected_repo_hash}");
	let tag_bytes = hex_decode(token_hmac_hex).map_err(|_| {
		MonochangeError::Config(
			"invalid CI auth token signature encoding: expected hexadecimal HMAC".to_string(),
		)
	})?;

	hmac::verify(&key, payload.as_bytes(), &tag_bytes).map_err(|_| {
		MonochangeError::Config(format!(
			"unauthorized CI auth token: HMAC signature verification failed for repository `{normalized_repo_id}`"
		))
	})
}

/// Split `mc_ci_<repo_hash>_<hmac>` into its hash and signature parts.
fn parse_repo_token(token: &str) -> MonochangeResult<(&str, &str)> {
	let trimmed = token.trim();
	let Some(rest) = trimmed.strip_prefix(TOKEN_PREFIX) else {
		return Err(MonochangeError::Config(
			"invalid CI auth token format: expected token starting with `mc_ci_`".to_string(),
		));
	};
	let Some(parts) = rest.split_once('_') else {
		return Err(MonochangeError::Config(
			"invalid CI auth token format: expected `mc_ci_<repo_hash>_<hmac>`".to_string(),
		));
	};

	Ok(parts)
}

/// Use `MONOCHANGE_CI_SECRET` when set, otherwise generate fresh entropy.
pub(super) fn configured_or_generated_secret() -> MonochangeResult<String> {
	match env_value("MONOCHANGE_CI_SECRET") {
		Some(secret) => Ok(secret),
		None => generate_random_secret_hex(),
	}
}

/// Generate a 32-byte random secret encoded as hex.
fn generate_random_secret_hex() -> MonochangeResult<String> {
	let rng = SystemRandom::new();
	let mut bytes = [0_u8; SECRET_BYTES];
	rng.fill(&mut bytes).map_err(|_| {
		MonochangeError::Io("failed to generate cryptographic secret for CI auth".to_string())
	})?;

	Ok(hex_encode(&bytes))
}

/// Encode bytes as lowercase hex.
fn hex_encode(bytes: &[u8]) -> String {
	let mut output = String::with_capacity(bytes.len() * 2);

	for byte in bytes {
		let _ = write!(output, "{byte:02x}");
	}

	output
}

/// Decode a lowercase or uppercase hex string into bytes.
fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
	if !hex.len().is_multiple_of(2) {
		return Err("odd hex length".to_string());
	}

	let mut bytes = Vec::with_capacity(hex.len() / 2);
	let chars = hex.as_bytes();
	let mut index = 0;

	while index < chars.len() {
		let pair =
			std::str::from_utf8(chars.get(index..index + 2).ok_or("slice")?).map_err(|_| "utf8")?;
		let value = u8::from_str_radix(pair, HEX_RADIX).map_err(|_| "invalid hex")?;

		bytes.push(value);
		index += 2;
	}

	Ok(bytes)
}

/// Path of the auth file inside a workspace root.
pub(super) fn ci_auth_file_path(root: &Path) -> PathBuf {
	root.join(CI_AUTH_RELATIVE_PATH)
}

/// Path of the memory store inside a workspace root.
fn ci_memory_file_path(root: &Path) -> PathBuf {
	root.join(CI_MEMORY_RELATIVE_PATH)
}

/// Load the auth file when it exists.
fn load_ci_auth_file(root: &Path) -> MonochangeResult<Option<CiAuthFile>> {
	let path = ci_auth_file_path(root);

	if !path.exists() {
		return Ok(None);
	}

	let content = fs::read_to_string(&path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;
	let parsed: CiAuthFile = serde_json::from_str(&content).map_err(|error| {
		MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
	})?;

	Ok(Some(parsed))
}

/// Persist the auth file with restrictive permissions.
pub(super) fn save_ci_auth_file(root: &Path, auth: &CiAuthFile) -> MonochangeResult<PathBuf> {
	let path = ci_auth_file_path(root);

	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
	}

	let serialized = serde_json::to_string_pretty(auth).map_err(|error| {
		MonochangeError::Io(format!("failed to serialize CI auth file: {error}"))
	})?;

	fs::write(&path, format!("{serialized}\n")).map_err(|error| {
		MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
	})?;

	#[cfg(unix)]
	restrict_file_permissions(&path)?;

	Ok(path)
}

/// Restrict the auth file to owner read and write.
#[cfg(unix)]
fn restrict_file_permissions(path: &Path) -> MonochangeResult<()> {
	use std::os::unix::fs::PermissionsExt;

	let permissions = fs::Permissions::from_mode(AUTH_FILE_MODE);

	fs::set_permissions(path, permissions).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to set 0600 permissions on {}: {error}",
			path.display()
		))
	})
}

/// Authenticate a caller against the current repository's canonical identity and secret.
pub fn authenticate_repo_access(
	root: &Path,
	repo_id_override: Option<&str>,
	explicit_token: Option<&str>,
) -> MonochangeResult<CiAuthReport> {
	let repo_id = derive_canonical_repo_id(root, repo_id_override);
	let repo_hash = compute_repo_hash(&repo_id);
	let auth_file_path = ci_auth_file_path(root);
	let auth_file = load_ci_auth_file(root)?;

	validate_auth_file_identity(auth_file.as_ref(), &auth_file_path, &repo_id)?;

	let secret = resolve_secret(auth_file.as_ref(), &repo_id)?;
	let (token, source) = resolve_token(explicit_token, auth_file.as_ref(), &repo_id, &secret);

	verify_repo_token(&token, &repo_id, &secret)?;

	Ok(CiAuthReport {
		action: "verify".to_string(),
		authenticated: true,
		repo_id,
		repo_hash,
		token,
		auth_file: auth_file_path.display().to_string(),
		source,
	})
}

/// Reject an auth file that was copied from a different repository.
fn validate_auth_file_identity(
	auth_file: Option<&CiAuthFile>,
	auth_file_path: &Path,
	repo_id: &str,
) -> MonochangeResult<()> {
	let Some(file_data) = auth_file else {
		return Ok(());
	};
	let file_repo_id = normalize_repo_identity(&file_data.repo_id);

	if file_repo_id == repo_id {
		return Ok(());
	}

	Err(MonochangeError::Config(format!(
		"repository authentication failed: {} belongs to `{file_repo_id}`, which does not match current repository `{repo_id}`",
		auth_file_path.display()
	)))
}

/// Resolve the shared secret from the environment or the auth file.
fn resolve_secret(auth_file: Option<&CiAuthFile>, repo_id: &str) -> MonochangeResult<String> {
	env_value("MONOCHANGE_CI_SECRET")
		.or_else(|| auth_file.map(|file_data| file_data.secret.clone()))
		.ok_or_else(|| {
			MonochangeError::Config(format!(
				"repository CI authentication is not initialized for `{repo_id}`; run `monochange ci auth init` or set `MONOCHANGE_CI_SECRET` / `MONOCHANGE_CI_TOKEN`"
			))
		})
}

/// Resolve which token to verify and where it came from.
fn resolve_token(
	explicit_token: Option<&str>,
	auth_file: Option<&CiAuthFile>,
	repo_id: &str,
	secret: &str,
) -> (String, String) {
	if let Some(token) = trimmed_non_empty(explicit_token) {
		return (token.to_string(), "flag".to_string());
	}

	if let Some(token) = env_value("MONOCHANGE_CI_TOKEN") {
		return (token, "MONOCHANGE_CI_TOKEN".to_string());
	}

	if let Some(file_data) = auth_file {
		return (file_data.token.clone(), CI_AUTH_RELATIVE_PATH.to_string());
	}

	(
		issue_repo_token(repo_id, secret),
		"MONOCHANGE_CI_SECRET".to_string(),
	)
}

/// Decide whether this invocation must prove repository ownership.
///
/// Requiring auth is sticky: once an auth file or token exists, every history
/// read or write must authenticate rather than silently trusting the checkout.
fn should_enforce_auth_for_context(
	root: &Path,
	require_auth_flag: bool,
	explicit_token: Option<&str>,
) -> bool {
	require_auth_flag
		|| explicit_token.is_some()
		|| std::env::var("MONOCHANGE_CI_TOKEN").is_ok_and(|value| !value.trim().is_empty())
		|| ci_auth_file_path(root).exists()
}

/// Load the memory store after validating the caller and the stored identity.
pub(super) fn load_ci_memory_store(
	root: &Path,
	repo_id_override: Option<&str>,
	require_auth: bool,
	explicit_token: Option<&str>,
) -> MonochangeResult<CiMemoryStore> {
	let repo_id = derive_canonical_repo_id(root, repo_id_override);
	let repo_hash = compute_repo_hash(&repo_id);

	if should_enforce_auth_for_context(root, require_auth, explicit_token) {
		authenticate_repo_access(root, repo_id_override, explicit_token)?;
	}

	let path = ci_memory_file_path(root);

	if !path.exists() {
		return Ok(CiMemoryStore::new(repo_id));
	}

	let store = read_memory_store(&path)?;

	validate_memory_identity(&store, &path, &repo_id, &repo_hash)?;

	Ok(store)
}

/// Read and deserialize the memory store.
fn read_memory_store(path: &Path) -> MonochangeResult<CiMemoryStore> {
	let content = fs::read_to_string(path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;

	serde_json::from_str(&content).map_err(|error| {
		MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
	})
}

/// Reject a memory store that belongs to a different repository.
fn validate_memory_identity(
	store: &CiMemoryStore,
	path: &Path,
	repo_id: &str,
	repo_hash: &str,
) -> MonochangeResult<()> {
	let stored_repo_id = normalize_repo_identity(&store.repo_id);

	if stored_repo_id == repo_id && store.repo_hash == repo_hash {
		return Ok(());
	}

	Err(MonochangeError::Config(format!(
		"CI memory repository mismatch: {} belongs to `{}` (`{}`), but current repository is `{repo_id}` (`{repo_hash}`)",
		path.display(),
		store.repo_id,
		store.repo_hash
	)))
}

/// Persist the memory store, creating `.monochange` when needed.
pub(super) fn save_ci_memory_store(
	root: &Path,
	store: &CiMemoryStore,
) -> MonochangeResult<PathBuf> {
	let path = ci_memory_file_path(root);

	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
	}

	let serialized = serde_json::to_string_pretty(store).map_err(|error| {
		MonochangeError::Io(format!("failed to serialize CI memory store: {error}"))
	})?;

	fs::write(&path, format!("{serialized}\n")).map_err(|error| {
		MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
	})?;

	Ok(path)
}

/// Read a non-empty environment variable.
fn env_value(name: &str) -> Option<String> {
	std::env::var(name)
		.ok()
		.map(|value| value.trim().to_string())
		.filter(|value| !value.is_empty())
}
