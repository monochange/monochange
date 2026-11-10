use super::*;

#[test]
fn signed_in_projects_render_with_the_runtime_style_parser() {
	let owner = leptos::prelude::Owner::new();
	owner.with(|| {
		let controller = RwSignal::new(PopupState::Loading);
		let ready = PopupReady {
			user: monochange_shared::SessionUser {
				github_id: 101,
				github_login: "alice".into(),
				github_avatar_url: None,
				plan_tier: "free".into(),
			},
			organizations: vec![monochange_shared::OrganizationSummary {
				login: "alice".into(),
				personal: true,
				avatar_url: None,
				supports_projects: true,
				repositories: vec![],
				projects: vec![monochange_shared::ProjectSummary {
					slug: "pocketbook".into(),
					name: "Pocketbook".into(),
					description: "Invoicing".into(),
					repository_count: 2,
				}],
			}],
			releases: vec![],
		};
		let screen = view! { <ReadyBlock ready=ready.clone() controller=controller /> };
		fn contains(element: &Element, text: &str) -> bool {
			element.props.text.as_deref() == Some(text)
				|| element.children.iter().any(|child| contains(child, text))
		}
		assert!(contains(&screen, "Pocketbook"));
		assert!(contains(&screen, "Open projects"));
		assert!(contains(&screen, "Sign out"));
		let options = view! { <SurfaceRoot for_sign_in=true state=PopupState::Ready(ready) controller=controller /> };
		assert!(contains(&options, "Connected as @alice"));
		assert!(contains(&options, "Sign out"));
	});
}
