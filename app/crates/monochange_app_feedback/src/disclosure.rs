//! The disclosure gate deciding what may leave the maintainers' side and be
//! shown to end users of the feedback portal.
//!
//! Two audiences see every feedback item:
//!
//! - Maintainer-facing artifacts (the `GitHub` issue, triage findings) live
//!   inside the repository, so they may contain full technical detail for both
//!   public and private repositories.
//! - User-facing artifacts (widget status updates, the public roadmap) are
//!   rendered through [`DisclosureGate::publish`]. For public repositories the
//!   draft passes through untouched. For private repositories internal paths,
//!   stack frames, internal URLs, and configuration values are redacted, issue
//!   and pull-request links are dropped, and technical detail is withheld, so
//!   the portal never reveals the codebase behind the product.

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::triage::ScreeningVerdict;
use crate::triage::screen_untrusted;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepositoryVisibility {
	Public,
	Private,
}

/// What an outbound update may reveal. Derived from repository visibility and
/// applied mechanically so no call site can forget it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisclosurePolicy {
	pub visibility: RepositoryVisibility,
	pub share_issue_links: bool,
	pub share_pr_links: bool,
	pub share_technical_detail: bool,
}

impl DisclosurePolicy {
	pub fn for_visibility(visibility: RepositoryVisibility) -> Self {
		match visibility {
			RepositoryVisibility::Public => {
				Self {
					visibility,
					share_issue_links: true,
					share_pr_links: true,
					share_technical_detail: true,
				}
			}
			RepositoryVisibility::Private => {
				Self {
					visibility,
					share_issue_links: false,
					share_pr_links: false,
					share_technical_detail: false,
				}
			}
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Why a piece of content must not leave the maintainers' side for a private
/// repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sensitivity {
	InternalPath,
	StackFrame,
	InternalUrl,
	Configuration,
}

/// Classifies one whitespace-separated token, or `None` when the token is safe
/// to publish. Deliberately literal: it is the defense-in-depth layer behind
/// the structured triage findings, not the only guard.
pub fn classify_token(token: &str) -> Option<Sensitivity> {
	let lower = token.to_ascii_lowercase();
	if let Some(rest) = lower
		.strip_prefix("http://")
		.or_else(|| lower.strip_prefix("https://"))
	{
		let authority = rest.split('/').next().unwrap_or_default();
		let host = authority.split(':').next().unwrap_or_default();
		let internal_host = host == "localhost"
			|| host.starts_with("internal.")
			|| host.starts_with("staging.")
			|| host.ends_with(".internal")
			|| host.contains(".corp");
		return if internal_host {
			Some(Sensitivity::InternalUrl)
		} else {
			None
		};
	}
	// Stack frames collapse to tokens such as `app/src/main.rs:41:9`.
	if [".rs:", ".ts:", ".tsx:", ".js:", ".py:", ".go:"]
		.into_iter()
		.any(|marker| lower.contains(marker))
	{
		return Some(Sensitivity::StackFrame);
	}
	let is_source_path = (lower.starts_with('/')
		|| lower.starts_with("./")
		|| lower.starts_with("src/")
		|| lower.starts_with("app/")
		|| lower.contains("/src/"))
		&& [
			".rs", ".ts", ".tsx", ".js", ".py", ".go", ".json", ".toml", ".yaml", ".yml",
		]
		.into_iter()
		.any(|extension| lower.ends_with(extension));
	if is_source_path {
		return Some(Sensitivity::InternalPath);
	}
	if ["api_key=", "apikey=", "secret=", "password=", "token="]
		.into_iter()
		.any(|marker| lower.contains(marker))
	{
		return Some(Sensitivity::Configuration);
	}
	None
}

/// Replaces every sensitive token with `[redacted]`, preserving spacing.
pub fn redact_internal_details(text: &str) -> String {
	text.split(' ')
		.map(|token| {
			match classify_token(token) {
				Some(_) => "[redacted]",
				None => token,
			}
		})
		.collect::<Vec<_>>()
		.join(" ")
}

#[derive(Debug, Clone, Copy)]
pub struct DisclosureGate;

impl DisclosureGate {
	/// Renders a user-facing update under the policy. Always re-screens the
	/// outbound text for untrusted instructions, even when nothing else is
	/// restricted, because outbound content is shown to other users.
	pub fn publish(
		draft: OutboundDraft,
		policy: DisclosurePolicy,
	) -> Result<OutboundUpdate, DisclosureError> {
		let screening_text = format!("{} {}", draft.title, draft.body);
		if let ScreeningVerdict::InjectionSuspected { markers } = screen_untrusted(&screening_text)
		{
			return Err(DisclosureError::UntrustedContent(markers.join(", ")));
		}
		let private = policy.visibility == RepositoryVisibility::Private;
		let title = if private {
			redact_internal_details(&draft.title)
		} else {
			draft.title
		};
		let body = if private {
			redact_internal_details(&draft.body)
		} else {
			draft.body
		};
		let technical_detail = if policy.share_technical_detail {
			draft.technical_detail.map(|detail| {
				if private {
					redact_internal_details(&detail)
				} else {
					detail
				}
			})
		} else {
			None
		};
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
			title,
			body,
			links,
			technical_detail,
		})
	}
}

#[cfg(test)]
#[path = "__tests__/disclosure_tests.rs"]
mod tests;
