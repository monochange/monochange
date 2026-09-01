use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PublishRateLimitReport;
use monochange_core::RateLimitOperation;
use monochange_core::RegistryKind;
use monochange_core::WorkspaceConfiguration;
use monochange_core::materialize_dependency_edges;
use monochange_publish::configured_package_publication_targets;
use monochange_publish::filter_pending_publish_requests;
use monochange_publish::plan_rate_limit_batches;
use monochange_publish::plan_rate_limit_window;
use monochange_publish::policies_for_rate_limit_operation;
use monochange_publish::render_rate_limit_window;

use crate::PreparedRelease;
use crate::discover_release_workspace;
use crate::package_publish;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum PublishRateLimitMode {
	Placeholder,
	Publish,
}

impl PublishRateLimitMode {
	#[must_use]
	pub(crate) fn operation(self) -> RateLimitOperation {
		match self {
			Self::Placeholder => RateLimitOperation::PlaceholderPublish,
			Self::Publish => RateLimitOperation::Publish,
		}
	}

	#[must_use]
	fn description(self) -> &'static str {
		match self {
			Self::Placeholder => "placeholder publish",
			Self::Publish => "publish",
		}
	}
}

pub(crate) async fn plan_publish_rate_limits(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	selected_packages: &BTreeSet<String>,
	mode: PublishRateLimitMode,
	publish_all_configured_packages: bool,
	dry_run: bool,
) -> MonochangeResult<PublishRateLimitReport> {
	let discovery = discover_release_workspace(root, configuration)?;
	let packages = &discovery.packages;
	let requests = if mode == PublishRateLimitMode::Placeholder {
		build_placeholder_plan_requests(root, configuration, packages, selected_packages).await?
	} else {
		build_release_plan_requests(
			root,
			configuration,
			prepared_release,
			packages,
			selected_packages,
			publish_all_configured_packages,
		)
		.await?
	};
	Ok(plan_publish_rate_limits_for_dependency_ordered_requests(
		&requests,
		packages,
		mode.operation(),
		dry_run,
	))
}

async fn build_placeholder_plan_requests(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	packages: &[monochange_core::PackageRecord],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<package_publish::PublishRequest>> {
	let requests = package_publish::build_placeholder_requests(
		root,
		configuration,
		packages,
		selected_packages,
	)?;
	filter_pending_publish_requests(&requests).await
}

async fn build_release_plan_requests(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	packages: &[monochange_core::PackageRecord],
	selected_packages: &BTreeSet<String>,
	publish_all_configured_packages: bool,
) -> MonochangeResult<Vec<package_publish::PublishRequest>> {
	let publications = if publish_all_configured_packages {
		configured_package_publication_targets(configuration, packages)
	} else {
		package_publish::release_record_package_publications_from_prepared_or_head(
			root,
			prepared_release,
		)
		.await?
	};
	let requests = package_publish::build_release_requests(
		configuration,
		packages,
		&publications,
		selected_packages,
	)?;
	filter_pending_publish_requests(&requests).await
}

pub(super) fn sort_requests_by_dependencies(
	requests: &mut [package_publish::PublishRequest],
	packages: &[monochange_core::PackageRecord],
) {
	use std::collections::BTreeMap;
	use std::collections::BTreeSet;
	use std::collections::VecDeque;

	let mut request_ids_by_record_id: BTreeMap<String, String> = BTreeMap::new();
	let request_ids: BTreeSet<String> = requests.iter().map(|r| r.package_id.clone()).collect();
	for package in packages {
		if request_ids.contains(&package.id) {
			request_ids_by_record_id.insert(package.id.clone(), package.id.clone());
			continue;
		}
		if let Some(request) = requests
			.iter()
			.find(|request| request.package_name == package.name)
		{
			request_ids_by_record_id.insert(package.id.clone(), request.package_id.clone());
		}
	}
	// Build graph: dependency_id -> list of dependent_ids
	let mut dependents: BTreeMap<String, Vec<String>> = BTreeMap::new();
	let mut in_degree: BTreeMap<String, usize> = BTreeMap::new();

	// Initialize all request IDs with in-degree 0
	for id in &request_ids {
		in_degree.insert(id.clone(), 0);
	}

	for mut edge in materialize_dependency_edges(packages) {
		let Some(from_package_id) = request_ids_by_record_id.get(&edge.from_package_id) else {
			continue;
		};
		edge.from_package_id.clone_from(from_package_id);
		let Some(to_package_id) = request_ids_by_record_id.get(&edge.to_package_id) else {
			continue;
		};
		edge.to_package_id.clone_from(to_package_id);

		// Build dependents for all edges where the target is in the request list,
		// so we can discover unselected dependents later.
		if request_ids.contains(&edge.to_package_id) {
			dependents
				.entry(edge.to_package_id.clone())
				.or_default()
				.push(edge.from_package_id.clone());
		}

		// Build in-degree only for edges between selected packages.
		if request_ids.contains(&edge.from_package_id) && request_ids.contains(&edge.to_package_id)
		{
			*in_degree.get_mut(&edge.from_package_id).unwrap() += 1;
		}
	}

	// Kahn's algorithm: start with packages that have no dependencies
	let mut queue: VecDeque<String> = in_degree
		.iter()
		.filter(|(_, deg)| **deg == 0)
		.map(|(id, _)| id.clone())
		.collect();

	let mut sorted_ids: Vec<String> = Vec::new();

	while let Some(id) = queue.pop_front() {
		sorted_ids.push(id.clone());
		if let Some(deps) = dependents.get(&id) {
			#[allow(clippy::single_match)]
			for dependent in deps {
				match in_degree.get_mut(dependent) {
					Some(degree) => {
						*degree -= 1;
						if *degree == 0 {
							queue.push_back(dependent.clone());
						}
					}
					None => {}
				}
			}
		}
	}

	// If cycle detected (sorted_ids.len() != requests.len()), keep original order
	if sorted_ids.len() != requests.len() {
		return;
	}

	// Map package_id -> index in sorted order
	let order_map: BTreeMap<&str, usize> = sorted_ids
		.iter()
		.enumerate()
		.map(|(idx, id)| (id.as_str(), idx))
		.collect();

	// Sort requests by their position in the topological order
	requests.sort_by_key(|req| {
		*order_map
			.get(req.package_id.as_str())
			.unwrap_or(&usize::MAX)
	});
}

pub(crate) fn plan_publish_rate_limits_for_dependency_ordered_requests(
	requests: &[package_publish::PublishRequest],
	packages: &[monochange_core::PackageRecord],
	operation: RateLimitOperation,
	dry_run: bool,
) -> PublishRateLimitReport {
	let mut requests = requests.to_vec();
	sort_requests_by_dependencies(&mut requests, packages);
	plan_publish_rate_limits_for_requests(&requests, operation, dry_run)
}

fn plan_publish_rate_limits_for_requests(
	requests: &[package_publish::PublishRequest],
	operation: RateLimitOperation,
	dry_run: bool,
) -> PublishRateLimitReport {
	let mut requests_by_registry =
		BTreeMap::<RegistryKind, Vec<&package_publish::PublishRequest>>::new();
	for request in requests {
		if request.mode == monochange_core::PublishMode::External {
			continue;
		}
		requests_by_registry
			.entry(request.registry)
			.or_default()
			.push(request);
	}

	let policies = policies_for_rate_limit_operation(operation)
		.into_iter()
		.map(|policy| (policy.registry, policy))
		.collect::<BTreeMap<_, _>>();

	let mut windows = Vec::new();
	let mut batches = Vec::new();

	for (registry, requests) in requests_by_registry {
		if let Some(policy) = policies.get(&registry) {
			let window = plan_rate_limit_window(policy, requests.len());
			batches.extend(plan_rate_limit_batches(policy, &requests));
			windows.push(window);
		}
	}

	windows.sort_by(|left, right| {
		left.registry
			.cmp(&right.registry)
			.then(left.operation.cmp(&right.operation))
	});
	batches.sort_by(|left, right| {
		left.registry
			.cmp(&right.registry)
			.then(left.batch_index.cmp(&right.batch_index))
	});

	let warnings = windows
		.iter()
		.filter(|window| !window.fits_single_window)
		.map(|window| {
			format!(
				"{} {} {} operations need {} batches under the current {} window",
				window.pending,
				window.registry,
				window.operation,
				window.batches_required,
				render_rate_limit_window(window.window_seconds)
			)
		})
		.collect();

	PublishRateLimitReport {
		dry_run,
		windows,
		batches,
		warnings,
	}
}

pub(crate) fn enforce_publish_rate_limits(
	configuration: &WorkspaceConfiguration,
	report: &PublishRateLimitReport,
	mode: PublishRateLimitMode,
) -> MonochangeResult<()> {
	let enforced_packages = report
		.batches
		.iter()
		.flat_map(|batch| batch.packages.iter())
		.any(|package| {
			configuration
				.package_by_id(package)
				.is_some_and(|definition| definition.publish.rate_limits.enforce)
		});
	if !enforced_packages {
		return Ok(());
	}

	let mut details = String::new();
	for window in report
		.windows
		.iter()
		.filter(|window| !window.fits_single_window)
	{
		if !details.is_empty() {
			details.push_str("; ");
		}
		let _ = write!(
			details,
			"{} {} {} packages={} batches={} window={}",
			mode.description(),
			window.registry,
			window.operation,
			window.pending,
			window.batches_required,
			render_rate_limit_window(window.window_seconds)
		);
	}
	if details.is_empty() {
		return Ok(());
	}

	Err(MonochangeError::Config(format!(
		"configured publish rate-limit enforcement blocked this run: {details}; use `monochange step plan-publish-rate-limits` to inspect batches or publish a filtered package subset"
	)))
}

#[cfg(test)]
#[path = "__tests__/publish_rate_limits_tests.rs"]
mod tests;
