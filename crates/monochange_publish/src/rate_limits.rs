use monochange_core::PublishRateLimitBatch;
use monochange_core::RateLimitConfidence;
use monochange_core::RateLimitEvidence;
use monochange_core::RateLimitEvidenceKind;
use monochange_core::RateLimitOperation;
use monochange_core::RegistryKind;
use monochange_core::RegistryRateLimitPolicy;
use monochange_core::RegistryRateLimitWindowPlan;

use crate::PublishRequest;

pub const CRATES_IO_SOURCE: &str = "https://github.com/rust-lang/crates.io";
pub const NPM_TRUST_DOCS: &str = "https://docs.npmjs.com/trusted-publishers";
pub const PUB_DEV_AUTOMATED_PUBLISHING: &str = "https://dart.dev/tools/pub/automated-publishing";
pub const JSR_PUBLISHING_DOCS: &str = "https://jsr.io/docs/publishing-packages";
pub const PYPI_TRUSTED_PUBLISHERS_DOCS: &str = "https://docs.pypi.org/trusted-publishers/";
pub fn plan_rate_limit_window(
	policy: &RegistryRateLimitPolicy,
	pending: usize,
) -> RegistryRateLimitWindowPlan {
	let batches_required = policy
		.limit
		.map_or(1, |limit| pending.div_ceil(limit as usize));
	let fits_single_window = policy.limit.is_none_or(|limit| pending <= limit as usize);

	RegistryRateLimitWindowPlan {
		registry: policy.registry,
		operation: policy.operation,
		limit: policy.limit,
		window_seconds: policy.window_seconds,
		pending,
		batches_required,
		fits_single_window,
		confidence: policy.confidence,
		notes: policy.notes.clone(),
		evidence: policy.evidence.clone(),
	}
}

pub fn plan_rate_limit_batches(
	policy: &RegistryRateLimitPolicy,
	requests: &[&PublishRequest],
) -> Vec<PublishRateLimitBatch> {
	let chunk_size = policy
		.limit
		.map_or_else(|| requests.len().max(1), |limit| limit as usize);
	let total_batches = requests.len().div_ceil(chunk_size).max(1);

	requests
		.chunks(chunk_size)
		.enumerate()
		.map(|(index, chunk)| {
			PublishRateLimitBatch {
				registry: policy.registry,
				operation: policy.operation,
				batch_index: index + 1,
				total_batches,
				packages: chunk
					.iter()
					.map(|request| request.package_id.clone())
					.collect(),
				recommended_wait_seconds: if index == 0 {
					None
				} else {
					policy.window_seconds.map(|seconds| seconds * index as u64)
				},
			}
		})
		.collect()
}

pub fn render_rate_limit_window(window_seconds: Option<u64>) -> String {
	match window_seconds {
		Some(86_400) => "24h".to_string(),
		Some(seconds) => format!("{seconds}s"),
		None => "unknown window".to_string(),
	}
}

pub fn policies_for_rate_limit_operation(
	operation: RateLimitOperation,
) -> Vec<RegistryRateLimitPolicy> {
	registry_rate_limit_policies()
		.into_iter()
		.map(|mut policy| {
			policy.operation = operation;
			policy
		})
		.collect()
}

pub fn registry_rate_limit_policies() -> Vec<RegistryRateLimitPolicy> {
	vec![
		RegistryRateLimitPolicy {
			registry: RegistryKind::CratesIo,
			operation: RateLimitOperation::Publish,
			limit: Some(10),
			window_seconds: Some(60),
			confidence: RateLimitConfidence::High,
			notes: "crates.io source enforces 10 uploads per minute for existing crates".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "crates.io application source".to_string(),
				url: CRATES_IO_SOURCE.to_string(),
				kind: RateLimitEvidenceKind::SourceCode,
				notes: "upload endpoint rate limiting in server implementation".to_string(),
			}],
		},
		RegistryRateLimitPolicy {
			registry: RegistryKind::Npm,
			operation: RateLimitOperation::Publish,
			limit: None,
			window_seconds: None,
			confidence: RateLimitConfidence::Low,
			notes: "npm does not publish a precise package publish quota; use sequential CI publishing with retries".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "npm trusted publishing documentation".to_string(),
				url: NPM_TRUST_DOCS.to_string(),
				kind: RateLimitEvidenceKind::Official,
				notes: "official workflow guidance but no exact package publish quota".to_string(),
			}],
		},
		RegistryRateLimitPolicy {
			registry: RegistryKind::Jsr,
			operation: RateLimitOperation::Publish,
			limit: Some(20),
			window_seconds: Some(86_400),
			confidence: RateLimitConfidence::High,
			notes: "JSR documents a daily publish limit per package scope".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "JSR publishing docs".to_string(),
				url: JSR_PUBLISHING_DOCS.to_string(),
				kind: RateLimitEvidenceKind::Official,
				notes: "official JSR publishing limits documentation".to_string(),
			}],
		},
		RegistryRateLimitPolicy {
			registry: RegistryKind::PubDev,
			operation: RateLimitOperation::Publish,
			limit: Some(12),
			window_seconds: Some(86_400),
			confidence: RateLimitConfidence::Medium,
			notes: "pub.dev community guidance consistently cites 12 publishes per day for new versions".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "Dart automated publishing docs".to_string(),
				url: PUB_DEV_AUTOMATED_PUBLISHING.to_string(),
				kind: RateLimitEvidenceKind::Official,
				notes: "official automation docs; limit itself is enforced operationally but not clearly enumerated on this page".to_string(),
			}],
		},
		RegistryRateLimitPolicy {
			registry: RegistryKind::Pypi,
			operation: RateLimitOperation::Publish,
			limit: None,
			window_seconds: None,
			confidence: RateLimitConfidence::Low,
			notes: "PyPI does not publish a precise package publish quota; use sequential CI publishing with retries".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "PyPI trusted publishers documentation".to_string(),
				url: PYPI_TRUSTED_PUBLISHERS_DOCS.to_string(),
				kind: RateLimitEvidenceKind::Official,
				notes: "official trusted-publisher workflow guidance but no exact package publish quota".to_string(),
			}],
		},
		RegistryRateLimitPolicy {
			registry: RegistryKind::GoProxy,
			operation: RateLimitOperation::Publish,
			limit: None,
			window_seconds: None,
			confidence: RateLimitConfidence::Low,
			notes: "Go modules are published by pushing VCS tags; the public proxy does not document a precise publish quota".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "Go module publishing reference".to_string(),
				url: "https://go.dev/ref/mod#publishing".to_string(),
				kind: RateLimitEvidenceKind::Official,
				notes: "official module publishing guidance documents tag-based publication".to_string(),
			}],
		},
	]
}
