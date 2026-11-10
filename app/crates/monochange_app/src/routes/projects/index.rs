//! `/projects` — the projects shell, not the marketing site.

use leptos::prelude::Get;
use leptos::prelude::Resource;
use leptoskit::prelude::*;

use crate::server_fns::organizations::list_organizations;
use crate::ui::heading;
use crate::ui::link;
use crate::ui::text;

pub fn page() -> Element {
	let organizations =
		crate::slots::once(|| Resource::new_blocking(|| (), |_| list_organizations()));
	let brand = heading(1, "monochange projects");
	match organizations.get() {
		None => {
			let loading = text("Loading projects…");
			leptoskit::view! {
				<View class="flex flex-col gap-4">
					{brand}
					{loading}
				</View>
			}
		}
		Some(Err(_)) => {
			let error = text("Projects couldn't be loaded. Please reload to try again.");
			leptoskit::view! {
				<View class="flex flex-col gap-4">
					{brand}
					{error}
				</View>
			}
		}
		Some(Ok(organizations)) if organizations.is_empty() => {
			let empty = heading(2, "Your next project starts here");
			let copy = text("Sign in and connect repositories to create a project.");
			let sign_in = link("/login?next=%2Fprojects", "Sign in");
			let connect = link("/dashboard", "Connect repositories");
			leptoskit::view! {
				<View class="flex flex-col gap-4">
					{brand}
					{empty}
					{copy}
					{sign_in}
					{connect}
				</View>
			}
		}
		Some(Ok(organizations)) => {
			let mut children = vec![brand, text("One place for the products you ship.")];
			for organization in organizations {
				children.push(link(organization.path(), &organization.login));
				children.push(text(format!("{} projects", organization.projects.len())));
				for project in organization.projects {
					children.push(link(
						monochange_shared::project_path(&organization.login, &project.slug),
						&project.name,
					));
					children.push(text(project.description));
				}
			}
			leptoskit::view! { <View class="flex flex-col gap-4">{children}</View> }
		}
	}
}
