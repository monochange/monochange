//! Feature-aware root resolution over the cargo member graph.
//!
//! A job that runs `cargo check -p <root> --no-default-features` does not
//! build the root's optional dependencies, so changes to them should not run
//! the job. This module resolves the closure cargo would actually build for
//! one root's feature selection: it starts from the selected features,
//! expands feature values (`"other_feature"`, `"dep:name"`, `"name/feat"`,
//! `"name?/feat"`, and the implicit feature an optional dependency gets),
//! counts a dependency edge only when it is non-optional or activated, and
//! unions features across every path into a member — Cargo's unification —
//! by iterating to a fixed point.
//!
//! The resolver is conservative by construction: anything it cannot resolve
//! falls back to the all-features closure with a warning, because unknown
//! must mean run, never skip. Two kinds of reference resolve to no workspace
//! edge instead: values naming dependencies outside the workspace
//! (`"dep:serde"`, `"serde/derive"` — external crates cannot depend on
//! workspace members) and same-package feature values (cargo validates those
//! while loading the manifest, so `cargo metadata` never serves an unknown
//! one).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use super::CargoDependency;
use super::CargoMember;
use super::WorkspaceGraph;

/// Name of the feature cargo enables for default-feature entries.
const DEFAULT_FEATURE: &str = "default";
/// Prefix marking a feature value that activates an optional dependency.
const DEP_PREFIX: &str = "dep:";

/// The resolved dependency closure of one feature-aware job root.
pub(super) struct FeatureClosure {
	/// Workspace members the selection builds (the root itself excluded,
	/// matching the plain-graph convention where owners check it directly).
	pub(super) packages: BTreeSet<String>,
	/// Fall-back-to-all-features explanations for the decision reason.
	pub(super) warnings: Vec<String>,
}

/// Resolve which workspace members `root` builds with `listed_features`
/// (plus its `default` feature when `default_features` is set).
///
/// The caller dispatches all-features roots to the plain graph walk, so the
/// selection here is always a concrete feature list.
pub(super) fn resolve_feature_closure(
	graph: &WorkspaceGraph,
	root: &str,
	listed_features: &[String],
	default_features: bool,
) -> FeatureClosure {
	let Some(root_member) = graph.cargo_member(root) else {
		return all_features_closure(
			graph,
			root,
			vec![format!(
				"root package `{root}` has no cargo feature metadata; following every dependency edge"
			)],
		);
	};

	for feature in listed_features {
		if !is_known_feature(root_member, feature) {
			return all_features_closure(
				graph,
				root,
				vec![format!(
					"feature `{feature}` is not defined by root package `{root}`; treating the root as all features"
				)],
			);
		}
	}

	let mut root_features: BTreeSet<String> = listed_features.iter().cloned().collect();

	if default_features && root_member.features.contains_key(DEFAULT_FEATURE) {
		root_features.insert(DEFAULT_FEATURE.to_string());
	}

	match fixed_point(graph, root, root_features) {
		Ok(packages) => {
			FeatureClosure {
				packages,
				warnings: Vec::new(),
			}
		}
		Err(warning) => all_features_closure(graph, root, vec![warning]),
	}
}

/// The conservative closure: every edge, with the fallback warning attached.
fn all_features_closure(
	graph: &WorkspaceGraph,
	root: &str,
	warnings: Vec<String>,
) -> FeatureClosure {
	FeatureClosure {
		packages: graph.transitive_dependencies(root),
		warnings,
	}
}

/// Iterate feature expansion and edge activation until nothing changes.
///
/// Every step only adds activations, so the loop terminates once every
/// member's feature and edge sets stop growing.
fn fixed_point(
	graph: &WorkspaceGraph,
	root: &str,
	root_features: BTreeSet<String>,
) -> Result<BTreeSet<String>, String> {
	let mut state: BTreeMap<String, MemberActivation> = BTreeMap::new();

	state.insert(
		root.to_string(),
		MemberActivation {
			features: root_features,
			active_deps: BTreeSet::new(),
		},
	);
	let mut packages = BTreeSet::new();
	let mut changed = true;

	while changed {
		changed = false;

		for member_name in state.keys().cloned().collect::<Vec<_>>() {
			let member = graph.cargo_member(&member_name).unwrap_or_else(|| {
				// patch-coverage:ignore-start -- the iteration set only ever holds cargo members: the
				// root is verified before the loop starts and every other member joins through
				// `activate`, which only runs for member-targeted dependency entries.
				unreachable!("activation state only tracks cargo members");
				// patch-coverage:ignore-end
			});
			let is_root_member = member_name == root;
			let current = state.get(&member_name).cloned().unwrap_or_default();

			// Non-optional eligible edges are always built.
			for dependency in &member.dependencies {
				if !dependency.optional
					&& dependency.kind.built_for(is_root_member)
					&& !current.active_deps.contains(&dependency.name)
				{
					changed |=
						activate(graph, &mut state, &mut packages, &member_name, dependency)?;
				}
			}

			// Expand every activated feature of this member.
			for feature in &current.features {
				if let Some(values) = member.features.get(feature) {
					for value in values {
						changed |= apply_feature_value(
							graph,
							&mut state,
							&mut packages,
							&member_name,
							member,
							value,
						)?;
					}
				} else {
					// patch-coverage:ignore-start -- cargo materializes implicit optional-dependency
					// features into the metadata table (`"opt_dep": ["dep:opt_dep"]`), so every
					// activated feature resolves through the table above. This branch only serves
					// metadata producers that omit implicit entries, where missing it would drop an
					// edge the selection really builds.
					if let Some(dependency) = member.dependencies.iter().find(|dependency| {
						&dependency.name == feature
							&& dependency.optional
							&& implicit_feature_visible(member, feature)
					}) {
						changed |=
							activate(graph, &mut state, &mut packages, &member_name, dependency)?;
					}
					// patch-coverage:ignore-end
				}
			}

			// Follow every active edge, unioning its features into the target.
			for dependency_name in &current.active_deps {
				// patch-coverage:ignore-start -- active_deps only receives names inserted by the
				// non-optional loop (kind-checked) or by feature syntax (normal/build dependencies
				// only: cargo forbids optional dev-dependencies), so the lookup always matches.
				if let Some(dependency) = member.dependencies.iter().find(|dependency| {
					&dependency.name == dependency_name && dependency.kind.built_for(is_root_member)
				}) {
					changed |=
						activate(graph, &mut state, &mut packages, &member_name, dependency)?;
				}
				// patch-coverage:ignore-end
			}
		}
	}

	Ok(packages)
}

/// Activation state accumulated for one member during resolution.
#[derive(Debug, Clone, Default)]
struct MemberActivation {
	/// Features activated on the member.
	features: BTreeSet<String>,
	/// Manifest keys of dependency entries whose edge is followed.
	active_deps: BTreeSet<String>,
}

/// Follow `dependency` from `dependent`, activating the edge and the features
/// the entry carries into its target.
///
/// Returns whether any activation grew.
fn activate(
	graph: &WorkspaceGraph,
	state: &mut BTreeMap<String, MemberActivation>,
	packages: &mut BTreeSet<String>,
	dependent: &str,
	dependency: &CargoDependency,
) -> Result<bool, String> {
	let mut changed = state
		.get_mut(dependent)
		.is_some_and(|activation| activation.active_deps.insert(dependency.name.clone()));

	let target_member = graph.cargo_member(&dependency.target).unwrap_or_else(|| {
		// patch-coverage:ignore-start -- dependency entries only exist for targets that resolved to
		// workspace members when the graph was built, so the lookup cannot miss here.
		unreachable!("dependency entries always target cargo members");
		// patch-coverage:ignore-end
	});

	changed |= packages.insert(dependency.target.clone());
	// The target joins the iteration set even when the entry enables nothing,
	// so its own non-optional edges still activate.
	state.entry(dependency.target.clone()).or_default();

	for feature in &dependency.features {
		if feature == DEFAULT_FEATURE {
			changed |= enable_default(state, &dependency.target, target_member);
		} else {
			changed |= enable_feature(state, &dependency.target, target_member, feature)?;
		}
	}

	if dependency.uses_default_features {
		changed |= enable_default(state, &dependency.target, target_member);
	}

	Ok(changed)
}

/// Apply one feature value (`dep:name`, `name/feat`, `name?/feat`, or a
/// same-package feature) to the activation state.
///
/// Values naming dependencies outside the workspace resolve to no edge:
/// external crates cannot depend on workspace members, so activating them
/// never changes the closure. Same-package values name features cargo
/// validated while loading the manifest.
fn apply_feature_value(
	graph: &WorkspaceGraph,
	state: &mut BTreeMap<String, MemberActivation>,
	packages: &mut BTreeSet<String>,
	member_name: &str,
	member: &CargoMember,
	value: &str,
) -> Result<bool, String> {
	if let Some(dependency_name) = value.strip_prefix(DEP_PREFIX) {
		let Some(dependency) = find_member_dependency(member, dependency_name) else {
			return Ok(false);
		};

		return activate(graph, state, packages, member_name, dependency);
	}

	if let Some((name_part, target_feature)) = value.split_once('/') {
		let conditional = name_part.ends_with('?');
		let dependency_name = name_part.trim_end_matches('?');
		let Some(dependency) = find_member_dependency(member, dependency_name) else {
			return Ok(false);
		};
		let mut changed = false;

		if !conditional {
			changed |= activate(graph, state, packages, member_name, dependency)?;
		}

		let edge_active = state
			.get(member_name)
			.is_some_and(|activation| activation.active_deps.contains(dependency_name));

		if edge_active && let Some(target_member) = graph.cargo_member(&dependency.target) {
			changed |= enable_feature(state, &dependency.target, target_member, target_feature)?;
		}

		return Ok(changed);
	}

	Ok(state
		.get_mut(member_name)
		.is_some_and(|activation| activation.features.insert(value.to_string())))
}

/// Enable a member's `default` feature when it defines one.
fn enable_default(
	state: &mut BTreeMap<String, MemberActivation>,
	target: &str,
	target_member: &CargoMember,
) -> bool {
	if !target_member.features.contains_key(DEFAULT_FEATURE) {
		return false;
	}

	state
		.entry(target.to_string())
		.or_default()
		.features
		.insert(DEFAULT_FEATURE.to_string())
}

/// Enable a named feature on a member, rejecting names the member does not
/// define.
///
/// `cargo metadata --no-deps` does not validate features that dependency
/// entries and `name/feat` values enable on their targets, so this fires on
/// real manifests; the closure falls back to all features with a warning.
fn enable_feature(
	state: &mut BTreeMap<String, MemberActivation>,
	target: &str,
	target_member: &CargoMember,
	feature: &str,
) -> Result<bool, String> {
	if !is_known_feature(target_member, feature) {
		return Err(format!(
			"feature `{feature}` is not defined by package `{target}`; treating the root as all features"
		));
	}

	Ok(state
		.entry(target.to_string())
		.or_default()
		.features
		.insert(feature.to_string()))
}

/// Find a dependency entry by its manifest key, ignoring entries whose
/// target is outside the workspace (those carry no member edges).
fn find_member_dependency<'a>(
	member: &'a CargoMember,
	dependency_name: &str,
) -> Option<&'a CargoDependency> {
	member
		.dependencies
		.iter()
		.find(|dependency| dependency.name == dependency_name)
}

/// Whether `feature` names an explicit feature or the implicit feature of an
/// optional dependency.
fn is_known_feature(member: &CargoMember, feature: &str) -> bool {
	member.features.contains_key(feature)
		// patch-coverage:ignore-start -- cargo materializes implicit optional-dependency features
		// into the metadata table, so the table lookup answers for it. The dependency scan only
		// serves metadata producers that omit implicit entries.
		|| member.dependencies.iter().any(|dependency| {
			dependency.name == feature
				&& dependency.optional
				&& implicit_feature_visible(member, feature)
		})
	// patch-coverage:ignore-end
}

/// Whether the implicit feature of optional dependency `name` still exists.
///
/// Referencing the dependency as `dep:name` anywhere in the feature table
/// removes the implicit feature; the `name/feat` forms do not.
// patch-coverage:ignore-start -- only reachable through the defensive paths above, which
// serve metadata producers that omit materialized implicit features.
fn implicit_feature_visible(member: &CargoMember, name: &str) -> bool {
	let hidden_name = format!("{DEP_PREFIX}{name}");

	!member
		.features
		.values()
		.flatten()
		.any(|value| *value == hidden_name)
}
// patch-coverage:ignore-end
