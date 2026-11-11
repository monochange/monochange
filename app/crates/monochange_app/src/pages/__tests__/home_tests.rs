//! Homepage SSR coverage for the "Pull one thread" release preview.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use super::*;
use crate::server_fns::auth::SessionUser;

fn sample_user() -> SessionUser {
	SessionUser {
		github_id: 101,
		github_login: "alice".to_owned(),
		github_avatar_url: None,
		plan_tier: "free".to_owned(),
	}
}

/// Fixture lookup so status tests read by package name.
fn package(id: &str) -> &'static Package {
	PACKAGES
		.iter()
		.find(|package| package.id == id)
		.expect("fixture package exists")
}

#[test]
fn workspace_href_opens_projects_and_preserves_the_login_destination() {
	assert_eq!(
		workspace_href_for(Some(Ok(Some(sample_user())))),
		"/projects"
	);
	assert_eq!(
		workspace_href_for(Some(Ok(None))),
		"/login?next=%2Fprojects"
	);
	assert_eq!(
		workspace_href_for(Some(Err(server_fn::ServerFnError::new("missing")))),
		"/login?next=%2Fprojects"
	);
	assert_eq!(workspace_href_for(None), "/login?next=%2Fprojects");
}

#[test]
fn bump_labels_and_semver_arithmetic_are_standard() {
	assert_eq!(Bump::Patch.label(), "patch");
	assert_eq!(Bump::Minor.label(), "minor");
	assert_eq!(Bump::Major.label(), "major");

	let current = (1, 8, 1);
	assert_eq!(bump_version(current, Bump::Patch), (1, 8, 2));
	assert_eq!(bump_version(current, Bump::Minor), (1, 9, 0));
	assert_eq!(bump_version(current, Bump::Major), (2, 0, 0));
	assert_eq!(format_version((2, 0, 0)), "2.0.0");
}

#[test]
fn changed_package_ids_match_the_workspace() {
	assert_eq!(ChangedPackage::Core.id(), "acme-core");
	assert_eq!(ChangedPackage::Sdk.id(), "@acme/sdk");
	assert_eq!(ChangedPackage::Mobile.id(), "acme_mobile");
}

#[test]
fn row_status_propagates_to_direct_dependents_only() {
	assert_eq!(
		row_status(package("acme-core"), ChangedPackage::Core, Bump::Minor),
		RowStatus::Changed(Bump::Minor)
	);
	assert_eq!(
		row_status(package("acme-cli"), ChangedPackage::Core, Bump::Minor),
		RowStatus::Dependent
	);
	assert_eq!(
		row_status(package("acme-py"), ChangedPackage::Core, Bump::Minor),
		RowStatus::Unchanged
	);

	assert_eq!(
		row_status(package("@acme/sdk"), ChangedPackage::Sdk, Bump::Major),
		RowStatus::Changed(Bump::Major)
	);
	assert_eq!(
		row_status(package("@acme/ui"), ChangedPackage::Sdk, Bump::Major),
		RowStatus::Dependent
	);
	assert_eq!(
		row_status(package("acme-core"), ChangedPackage::Sdk, Bump::Major),
		RowStatus::Unchanged
	);

	assert_eq!(
		row_status(package("acme_mobile"), ChangedPackage::Mobile, Bump::Patch),
		RowStatus::Changed(Bump::Patch)
	);
	assert_eq!(
		row_status(package("acme-cli"), ChangedPackage::Mobile, Bump::Patch),
		RowStatus::Unchanged
	);

	assert!(affected(package("acme-cli"), ChangedPackage::Core));
	assert!(!affected(package("@acme/ui"), ChangedPackage::Core));
}

#[test]
fn plan_summary_counts_the_changed_package_and_its_dependents() {
	assert_eq!(
		plan_summary(ChangedPackage::Core),
		"2 of 6 packages release · nothing published yet"
	);
	assert_eq!(
		plan_summary(ChangedPackage::Sdk),
		"2 of 6 packages release · nothing published yet"
	);
	assert_eq!(
		plan_summary(ChangedPackage::Mobile),
		"1 of 6 packages release · nothing published yet"
	);
}

#[test]
fn changeset_slugs_and_notes_follow_the_selected_package() {
	assert_eq!(changeset_slug(ChangedPackage::Core), "quiet-otters");
	assert_eq!(changeset_slug(ChangedPackage::Sdk), "brave-lanterns");
	assert_eq!(changeset_slug(ChangedPackage::Mobile), "tidy-comets");

	assert_eq!(
		changeset_note(ChangedPackage::Core),
		"Faster dependency resolution"
	);
	assert_eq!(changeset_note(ChangedPackage::Sdk), "Typed release events");
	assert_eq!(
		changeset_note(ChangedPackage::Mobile),
		"Offline release notes"
	);
}

#[test]
fn next_versions_use_the_row_status_bump() {
	assert_eq!(
		next_version_text(package("acme-core"), RowStatus::Changed(Bump::Major)),
		"2.0.0"
	);
	assert_eq!(
		next_version_text(package("acme-cli"), RowStatus::Dependent),
		"2.0.5"
	);
	assert_eq!(
		next_version_text(package("acme-py"), RowStatus::Unchanged),
		"—"
	);
}

#[test]
fn row_styles_highlight_changed_rows_and_fade_unchanged_ones() {
	let changed = row_style(RowStatus::Changed(Bump::Minor));
	assert!(changed.row.contains("bg-brand-400/10"));
	assert!(changed.leading.contains("border-l-2"));
	assert!(changed.next.contains("text-brand-300"));
	assert_eq!(changed.tag_label, Some("minor"));

	let dependent = row_style(RowStatus::Dependent);
	assert!(dependent.row.contains("bg-brand-400/5"));
	assert!(dependent.next.contains("text-brand-300"));
	assert_eq!(dependent.tag_label, Some("dependency"));

	let unchanged = row_style(RowStatus::Unchanged);
	assert!(unchanged.row.contains("opacity-45"));
	assert_eq!(unchanged.tag_label, None);
}

#[test]
fn pill_classes_mark_the_pressed_choice() {
	assert!(pill_class(true).contains("bg-brand-300"));
	assert!(pill_class(true).contains("text-brand-950"));
	assert!(pill_class(false).contains("text-hero-muted"));
}

#[test]
fn console_setters_update_both_signals() {
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap();
	runtime.block_on(async {
		Owner::new().with(|| {
			let (selected, set_selected) = signal(ChangedPackage::Core);
			let (bump, set_bump) = signal(Bump::Minor);
			assert_eq!(selected.get(), ChangedPackage::Core);
			assert_eq!(bump.get(), Bump::Minor);

			select_package(set_selected, ChangedPackage::Sdk);
			select_bump(set_bump, Bump::Major);
			assert_eq!(selected.get(), ChangedPackage::Sdk);
			assert_eq!(bump.get(), Bump::Major);
		});
	});
}

#[tokio::test]
async fn home_page_renders_the_release_preview() {
	let html = crate::tests::render(|| view! { <HomePage /> }).await;

	assert!(html.contains("Change one package. See every release it touches."));
	assert!(html.contains("Three commands. One decision."));
	assert!(html.contains("Your tools stay. The plan is shared."));
	assert!(html.contains("One sign-in. Every surface."));
	assert!(html.contains("Hey, this is free."));
	assert!(html.contains(".changeset/quiet-otters.md"));
	assert!(html.contains("acme-core"));
	assert!(html.contains("@acme/ui"));
	assert!(html.contains("monochange preview"));
	assert!(html.contains("2 of 6 packages release"));
	assert!(
		html.contains("href=\"/login?next=%2Fprojects\"") || html.contains("href=\"/projects\"")
	);
}
