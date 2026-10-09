//! The disclosure rules deciding what a piece of text may reveal, and the
//! gate every user-facing update passes through.
//!
//! Text produced by the pipeline lands on one of two [`Surface`]s:
//!
//! - The **portal** — the widget, notifications, the public roadmap — is read
//!   by end users of the integrated app.
//! - The **repository** — the `GitHub` issue, the agent brief, the pull
//!   request — is read by whoever can read the repository.
//!
//! What may appear depends on the surface and the repository's visibility:
//!
//! | Sensitivity                   | Portal, public repo | Portal, private repo | Repository, public | Repository, private |
//! | ----------------------------- | ------------------- | -------------------- | ------------------ | ------------------- |
//! | Internal path, stack frame    | shown               | redacted             | shown              | shown               |
//! | Internal URL or address       | redacted            | redacted             | redacted           | shown               |
//! | Secret, personal data         | redacted            | redacted             | redacted           | redacted            |
//!
//! A public repository's code is already public, so naming a file reveals
//! nothing. Its deployment hosts are not, so internal URLs stay hidden
//! wherever the world can read. Secrets and personal data never belong in a
//! generated artifact, whoever reads it.

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::triage::ScreeningVerdict;
use crate::triage::screen_untrusted;

/// Replacement text for redacted tokens.
pub const REDACTED: &str = "[redacted]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryVisibility {
	Public,
	Private,
}

/// Who reads a piece of generated text. See the module docs for the matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
	Portal,
	Repository,
}

/// Why a token must not appear on some surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
	InternalPath,
	StackFrame,
	InternalUrl,
	Secret,
	PersonalData,
}

impl Sensitivity {
	/// Whether a token of this sensitivity may appear on `surface` for a
	/// repository with `visibility`.
	pub fn allowed_on(self, surface: Surface, visibility: RepositoryVisibility) -> bool {
		match self {
			Sensitivity::Secret | Sensitivity::PersonalData => false,
			Sensitivity::InternalUrl => {
				surface == Surface::Repository && visibility == RepositoryVisibility::Private
			}
			Sensitivity::InternalPath | Sensitivity::StackFrame => {
				surface == Surface::Repository || visibility == RepositoryVisibility::Public
			}
		}
	}

	pub fn label(self) -> &'static str {
		match self {
			Sensitivity::InternalPath => "internal path",
			Sensitivity::StackFrame => "stack frame",
			Sensitivity::InternalUrl => "internal address",
			Sensitivity::Secret => "secret",
			Sensitivity::PersonalData => "personal data",
		}
	}
}

/// What an outbound portal update may reveal beyond the redaction matrix.
/// Derived from repository visibility so no call site can forget it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisclosurePolicy {
	pub visibility: RepositoryVisibility,
	pub share_issue_links: bool,
	pub share_pr_links: bool,
	pub share_technical_detail: bool,
}

impl DisclosurePolicy {
	pub fn for_visibility(visibility: RepositoryVisibility) -> Self {
		let public = visibility == RepositoryVisibility::Public;
		Self {
			visibility,
			share_issue_links: public,
			share_pr_links: public,
			share_technical_detail: public,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "url", rename_all = "snake_case")]
pub enum PublicLink {
	Issue(String),
	PullRequest(String),
	/// Release notes are an explicitly published artifact, so they stay
	/// shareable even for private repositories.
	ReleaseNotes(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundDraft {
	pub title: String,
	pub body: String,
	pub links: Vec<PublicLink>,
	pub technical_detail: Option<String>,
}

/// A portal update that has passed the gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundUpdate {
	pub title: String,
	pub body: String,
	pub links: Vec<PublicLink>,
	pub technical_detail: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DisclosureError {
	#[error("outbound content contains untrusted instructions: {0}")]
	UntrustedContent(String),
}

/// Text after redaction, plus what was removed so maintainers can see why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redaction {
	pub text: String,
	pub removed: Vec<Sensitivity>,
}

/// Classifies one token, or returns `None` when the token is safe everywhere.
/// The token should already be stripped of surrounding punctuation; use
/// [`sensitive_tokens`] or [`redact`] on free text.
///
/// Deliberately literal: it is the defense-in-depth layer behind structured
/// triage findings, not the only guard.
pub fn classify_token(token: &str) -> Option<Sensitivity> {
	if token.is_empty() {
		return None;
	}
	if is_secret(token) {
		return Some(Sensitivity::Secret);
	}
	let lower = token.to_ascii_lowercase();
	if let Some(rest) = lower
		.strip_prefix("http://")
		.or_else(|| lower.strip_prefix("https://"))
	{
		let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
		let host_and_port = authority.rsplit('@').next().unwrap_or_default();
		let host = host_and_port.split(':').next().unwrap_or_default();
		return is_internal_host(host).then_some(Sensitivity::InternalUrl);
	}
	if is_email(&lower) {
		return Some(Sensitivity::PersonalData);
	}
	if is_private_address(lower.split(':').next().unwrap_or_default()) {
		return Some(Sensitivity::InternalUrl);
	}
	if is_stack_frame(&lower) {
		return Some(Sensitivity::StackFrame);
	}
	if is_source_path(&lower) {
		return Some(Sensitivity::InternalPath);
	}
	None
}

const SOURCE_EXTENSIONS: [&str; 22] = [
	".rs", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".py", ".go", ".rb", ".java", ".kt", ".swift",
	".dart", ".cs", ".php", ".sql", ".json", ".toml", ".yaml", ".yml", ".env", ".lock",
];

const SECRET_ASSIGNMENTS: [&str; 8] = [
	"api_key=",
	"apikey=",
	"access_key=",
	"secret=",
	"client_secret=",
	"password=",
	"passwd=",
	"token=",
];

/// Prefixes of credentials issued by common providers, matched
/// case-sensitively because the providers define them that way.
const SECRET_PREFIXES: [&str; 12] = [
	"ghp_",
	"gho_",
	"ghu_",
	"ghs_",
	"ghr_",
	"github_pat_",
	"sk-",
	"sk_live_",
	"sk_test_",
	"xoxb-",
	"xoxp-",
	"glpat-",
];

fn is_secret(token: &str) -> bool {
	let lower = token.to_ascii_lowercase();
	if SECRET_ASSIGNMENTS
		.into_iter()
		.any(|marker| lower.contains(marker))
	{
		return true;
	}
	if token.len() >= 20
		&& SECRET_PREFIXES
			.into_iter()
			.any(|prefix| token.starts_with(prefix))
	{
		return true;
	}
	// AWS access key ids: `AKIA` followed by 16 upper-case alphanumerics.
	if token.len() == 20
		&& token.starts_with("AKIA")
		&& token[4..]
			.chars()
			.all(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
	{
		return true;
	}
	// JSON web tokens: three base64url segments, the first a JSON header.
	token.starts_with("eyJ") && token.split('.').count() == 3 && token.len() >= 30
}

/// Host suffixes reserved for private networks. `host` is already lower-case.
const INTERNAL_HOST_SUFFIXES: [&str; 3] = [".internal", ".local", ".corp"];

fn is_internal_host(host: &str) -> bool {
	host == "localhost"
		|| is_private_address(host)
		|| host.starts_with("internal.")
		|| host.starts_with("staging.")
		|| INTERNAL_HOST_SUFFIXES
			.into_iter()
			.any(|suffix| host.ends_with(suffix))
		|| host.contains(".corp.")
}

/// Loopback and RFC 1918 IPv4 addresses.
fn is_private_address(candidate: &str) -> bool {
	let octets: Vec<u8> = candidate
		.split('.')
		.map_while(|part| part.parse().ok())
		.collect();
	if octets.len() != 4 || candidate.split('.').count() != 4 {
		return false;
	}
	matches!(
		(octets[0], octets[1]),
		(10 | 127, _) | (192, 168) | (172, 16..=31)
	)
}

fn is_email(lower: &str) -> bool {
	let Some((local, domain)) = lower.split_once('@') else {
		return false;
	};
	!local.is_empty()
		&& !domain.starts_with('.')
		&& domain.contains('.')
		&& !domain.ends_with('.')
		&& !domain.contains('@')
}

/// Stack frames collapse to tokens such as `app/src/main.rs:41:9`: a source
/// extension immediately followed by `:<line>`.
fn is_stack_frame(lower: &str) -> bool {
	SOURCE_EXTENSIONS.into_iter().any(|extension| {
		lower.match_indices(extension).any(|(index, _)| {
			let rest = &lower[index + extension.len()..];
			rest.strip_prefix(':')
				.is_some_and(|line| line.starts_with(|character: char| character.is_ascii_digit()))
		})
	})
}

fn is_source_path(lower: &str) -> bool {
	let looks_like_path = lower.starts_with('/')
		|| lower.starts_with("./")
		|| lower.starts_with("../")
		|| lower.starts_with("~/")
		|| lower.starts_with("src/")
		|| lower.starts_with("app/")
		|| lower.starts_with("lib/")
		|| lower.contains("/src/")
		|| lower.contains('\\');
	looks_like_path
		&& SOURCE_EXTENSIONS
			.into_iter()
			.any(|extension| lower.ends_with(extension))
}

/// Characters that wrap a token in prose without being part of it.
fn is_leading_wrapper(character: char) -> bool {
	matches!(character, '(' | '[' | '{' | '<' | '"' | '\'' | '`')
}

fn is_trailing_wrapper(character: char) -> bool {
	matches!(
		character,
		')' | ']' | '}' | '>' | '"' | '\'' | '`' | ',' | ';' | ':' | '.' | '!' | '?'
	)
}

/// Splits a whitespace-free word into wrapper prefix, core token, and wrapper
/// suffix, so `(/app/src/billing.rs),` classifies as `/app/src/billing.rs`.
fn split_wrappers(word: &str) -> (&str, &str, &str) {
	let core_start = word.len() - word.trim_start_matches(is_leading_wrapper).len();
	let without_prefix = &word[core_start..];
	let core_end = without_prefix.trim_end_matches(is_trailing_wrapper).len();
	(
		&word[..core_start],
		&without_prefix[..core_end],
		&without_prefix[core_end..],
	)
}

/// Walks `text` word by word, preserving all whitespace exactly, and lets
/// `visit` decide what to emit for each word's core token.
fn rewrite_words(text: &str, mut visit: impl FnMut(&str) -> Option<String>) -> String {
	let mut output = String::with_capacity(text.len());
	let mut rest = text;
	while !rest.is_empty() {
		let word_start = rest
			.find(|character: char| !character.is_whitespace())
			.unwrap_or(rest.len());
		output.push_str(&rest[..word_start]);
		rest = &rest[word_start..];
		let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
		let (prefix, core, suffix) = split_wrappers(&rest[..word_end]);
		output.push_str(prefix);
		match visit(core) {
			Some(replacement) => output.push_str(&replacement),
			None => output.push_str(core),
		}
		output.push_str(suffix);
		rest = &rest[word_end..];
	}
	output
}

/// Every sensitive token in `text`, in order of appearance.
pub fn sensitive_tokens(text: &str) -> Vec<(String, Sensitivity)> {
	let mut found = Vec::new();
	rewrite_words(text, |core| {
		if let Some(sensitivity) = classify_token(core) {
			found.push((core.to_owned(), sensitivity));
		}
		None
	});
	found
}

/// Replaces every token that may not appear on `surface` with [`REDACTED`],
/// preserving whitespace and surrounding punctuation.
pub fn redact(text: &str, surface: Surface, visibility: RepositoryVisibility) -> Redaction {
	let mut removed = Vec::new();
	let text = rewrite_words(text, |core| {
		let sensitivity = classify_token(core)?;
		if sensitivity.allowed_on(surface, visibility) {
			return None;
		}
		removed.push(sensitivity);
		Some(REDACTED.to_owned())
	});
	Redaction { text, removed }
}

#[derive(Debug, Clone, Copy)]
pub struct DisclosureGate;

impl DisclosureGate {
	/// Renders a portal update under the policy. Always re-screens the
	/// outbound text for untrusted instructions, because portal content is
	/// shown to other users and a buggy or hostile triage engine must not be
	/// able to relay instructions through it.
	pub fn publish(
		draft: OutboundDraft,
		policy: DisclosurePolicy,
	) -> Result<OutboundUpdate, DisclosureError> {
		let screening_text = format!("{} {}", draft.title, draft.body);
		if let ScreeningVerdict::InjectionSuspected { markers } = screen_untrusted(&screening_text)
		{
			return Err(DisclosureError::UntrustedContent(markers.join(", ")));
		}
		let portal = |text: &str| redact(text, Surface::Portal, policy.visibility).text;
		let technical_detail = draft
			.technical_detail
			.filter(|_| policy.share_technical_detail)
			.map(|detail| portal(&detail));
		let links = draft
			.links
			.into_iter()
			.filter(|link| {
				match link {
					PublicLink::Issue(_) => policy.share_issue_links,
					PublicLink::PullRequest(_) => policy.share_pr_links,
					PublicLink::ReleaseNotes(_) => true,
				}
			})
			.collect();
		Ok(OutboundUpdate {
			title: portal(&draft.title),
			body: portal(&draft.body),
			links,
			technical_detail,
		})
	}
}

#[cfg(test)]
#[path = "__tests__/disclosure_tests.rs"]
mod tests;
