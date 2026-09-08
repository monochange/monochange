//! Trusted-publishing readiness checks for the publish-readiness step.
//!
//! These checks run before any registry mutation happens. They verify the
//! project-side trusted-publishing configuration for every selected package
//! and probe the registry-side package state where a public API allows it:
//!
//! - **Project side**: the GitHub trust context (repository, workflow,
//!   environment) must resolve, the referenced workflow file must exist, and
//!   the current environment must be able to verify the CI/OIDC identity.
//! - **Registry side**: npm, crates.io, and pub.dev only accept trusted
//!   publishing for packages that already exist on the registry (the first
//!   publish of a package cannot use trusted publishing), so readiness probes
//!   the registry and blocks packages that were never published.
//!
//! Registry-side setup (the trusted publisher entries themselves) cannot be
//! read back without registry credentials, so those cases surface as
//! `ManualVerificationRequired` with the registry setup URL instead of a
//! hard block.

use std::collections::BTreeMap;
use std::path::Path;

use monochange_core::PublishRegistry;
use monochange_core::SourceConfiguration;
use monochange_github::resolve_github_trust_context;
use monochange_publish::CiProviderKind;
use monochange_publish::Client;
use monochange_publish::PublishRequest;
use monochange_publish::RegistryEndpoints;
use monochange_publish::detect_trusted_publishing_identity;
use monochange_publish::manual_setup_url;
use monochange_publish::provider_registry_trust_capability;
use monochange_publish::registry_client;
use monochange_publish::registry_package_exists_with_transport;
use monochange_publish::trusted_publishing_capability_message;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TrustedPublishingReadinessStatus {
	Disabled,
	Verified,
	ManualVerificationRequired,
	Blocked,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct TrustedPublishingReadiness {
	pub status: TrustedPublishingReadinessStatus,
	pub message: String,
}

pub(crate) async fn check_trusted_publishing_readiness(
	root: &Path,
	source: Option<&SourceConfiguration>,
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
) -> TrustedPublishingReadiness {
	if !request.trusted_publishing.enabled {
		return TrustedPublishingReadiness {
			status: TrustedPublishingReadinessStatus::Disabled,
			message: "trusted publishing is disabled; publishing will use registry tokens"
				.to_string(),
		};
	}

	if let Some(blocker) = project_side_blocker(root, source, request, env_map) {
		return blocker;
	}

	let client = match registry_client() {
		Ok(client) => client,
		Err(error) => {
			return manual_verification_readiness(
				"trusted publishing registry state could not be checked",
				&error.render(),
				&manual_setup_url(request),
			);
		}
	};
	let endpoints = RegistryEndpoints::from_env();
	registry_side_readiness(request, &client, &endpoints).await
}

/// Sync project-side trusted-publishing blocker used by the publish-run
/// preflight registry: returns a blocked message when the package's trust
/// configuration cannot support a built-in release publish from this
/// environment, or `None` when the package may proceed.
pub(crate) fn trusted_publishing_project_blocker_message(
	root: &Path,
	source: Option<&SourceConfiguration>,
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
) -> Option<String> {
	if !request.trusted_publishing.enabled {
		return None;
	}
	project_side_blocker(root, source, request, env_map)
		.filter(|readiness| readiness.status == TrustedPublishingReadinessStatus::Blocked)
		.map(|readiness| readiness.message)
}

fn project_side_blocker(
	root: &Path,
	source: Option<&SourceConfiguration>,
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
) -> Option<TrustedPublishingReadiness> {
	let registry = PublishRegistry::Builtin(request.registry);
	let identity = detect_trusted_publishing_identity(env_map);
	let capability = provider_registry_trust_capability(&registry, identity.provider());
	let capability_message = trusted_publishing_capability_message(&registry, &identity);

	if identity.provider() == CiProviderKind::Unknown {
		// Outside a supported CI provider the publish-time enforcement would
		// reject the publish, but locally this is expected: surface it as a
		// manual verification requirement instead of a hard blocker so the
		// readiness report stays useful during development.
		return Some(TrustedPublishingReadiness {
			status: TrustedPublishingReadinessStatus::ManualVerificationRequired,
			message: format!(
				"trusted publishing identity could not be verified in this environment; built-in release publishing must run from the configured CI workflow. {capability_message}"
			),
		});
	}

	if !capability.trusted_publishing || !capability.ci_identity_verifiable {
		return Some(blocked_trust_readiness(
			request,
			&format!(
				"trusted publishing is not supported for {} from {}; set `publish.trusted_publishing = false` to opt out. {capability_message}",
				request.registry,
				identity.provider().label()
			),
		));
	}

	if !identity.is_verifiable_by_env() {
		// Inside a supported CI provider a missing OIDC identity fails the
		// publish outright, so block now; the local-run case is handled by the
		// `CiProviderKind::Unknown` branch above.
		return Some(blocked_trust_readiness(
			request,
			&format!(
				"trusted publishing publish-time environment is incomplete; built-in release publishing would be rejected. {capability_message}"
			),
		));
	}

	match resolve_github_trust_context(root, source, &request.trusted_publishing, env_map) {
		Ok(context) => {
			let workflow_path = root
				.join(".github")
				.join("workflows")
				.join(&context.workflow);
			if !workflow_path.is_file() {
				return Some(blocked_trust_readiness(
					request,
					&format!(
						"trusted publishing workflow `{}` does not exist at {}; fix `publish.trusted_publishing.workflow`",
						context.workflow,
						workflow_path.display()
					),
				));
			}
			None
		}
		Err(error) => {
			Some(blocked_trust_readiness(
				request,
				&format!("trusted publishing setup is incomplete: {error}. {capability_message}"),
			))
		}
	}
}

fn blocked_trust_readiness(request: &PublishRequest, reason: &str) -> TrustedPublishingReadiness {
	TrustedPublishingReadiness {
		status: TrustedPublishingReadinessStatus::Blocked,
		message: format!(
			"{reason}; open {} to finish trusted publishing setup, or set `publish.trusted_publishing = false` to opt out",
			manual_setup_url(request)
		),
	}
}

pub(crate) async fn registry_side_readiness(
	request: &PublishRequest,
	client: &Client,
	endpoints: &RegistryEndpoints,
) -> TrustedPublishingReadiness {
	let setup_url = manual_setup_url(request);
	match registry_package_exists_with_transport(request, client, endpoints).await {
		Ok(Some(true)) => {
			TrustedPublishingReadiness {
				status: TrustedPublishingReadinessStatus::ManualVerificationRequired,
				message: format!(
					"package exists on {}; verify the registry-side trusted publisher configuration at {setup_url} (monochange cannot read trusted publisher entries without registry credentials)",
					request.registry
				),
			}
		}
		Ok(Some(false)) => {
			TrustedPublishingReadiness {
				status: TrustedPublishingReadinessStatus::Blocked,
				message: format!(
					"`{}` has never been published to {} and trusted publishing can only be configured after an initial publish; bootstrap the package with `monochange step placeholder-publish`, then rerun `monochange step publish-readiness --from HEAD --output <PATH>`",
					request.package_name, request.registry
				),
			}
		}
		Ok(None) => {
			manual_verification_readiness(
				"trusted publishing is not probe-able for this registry without credentials",
				setup_url.as_str(),
				&setup_url,
			)
		}
		Err(error) => {
			manual_verification_readiness(
				"trusted publishing registry lookup failed",
				&error.render(),
				&setup_url,
			)
		}
	}
}

fn manual_verification_readiness(
	reason: &str,
	detail: &str,
	setup_url: &str,
) -> TrustedPublishingReadiness {
	TrustedPublishingReadiness {
		status: TrustedPublishingReadinessStatus::ManualVerificationRequired,
		message: format!(
			"{reason}: {detail}; verify the registry-side trusted publishing setup manually at {setup_url}"
		),
	}
}

#[cfg(test)]
#[allow(clippy::disallowed_methods, clippy::cloned_ref_to_slice_refs)]
#[path = "__tests__/trusted_publishing_readiness_tests.rs"]
mod tests;
