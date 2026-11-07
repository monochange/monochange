use monochange_extension_shared::OrganizationSummary;
use monochange_extension_shared::ProjectSummary;
use monochange_extension_shared::SessionUser;
use monochange_extension_shared::WebsiteRelease;

use crate::chrome::CHROME_CLASSES;
use crate::chrome::MC_ACCENT_LIGHT;
use crate::chrome::MC_PAGE_LIGHT;
use crate::chrome::REQUIRED_MC_TOKENS;
use crate::chrome::css_defines_mc_tokens;
use crate::popup::PopupReady;
use crate::popup::PopupState;
use crate::popup::PopupWireState;
use crate::popup::ProjectRow;
use crate::session::bearer_from_kept;

#[test]
fn app_css_publishes_every_release_floor_token() {
	let css = include_str!("../../../assets/app.css");
	assert!(
		css_defines_mc_tokens(css),
		"assets/app.css must define {:?}",
		REQUIRED_MC_TOKENS
	);
	assert!(
		css.to_ascii_lowercase()
			.contains(&MC_PAGE_LIGHT.to_ascii_lowercase())
	);
	assert!(
		css.to_ascii_lowercase()
			.contains(&MC_ACCENT_LIGHT.to_ascii_lowercase())
	);
	for class in CHROME_CLASSES {
		assert!(css.contains(class), "assets/app.css must style .{class}");
	}
}

#[test]
fn bearer_accepts_raw_and_json_session_shapes() {
	assert_eq!(bearer_from_kept("  tok_abc  ").as_deref(), Some("tok_abc"));
	assert_eq!(
		bearer_from_kept(r#"{"access_token":"pair","refresh_token":"r"}"#).as_deref(),
		Some("pair")
	);
	assert_eq!(bearer_from_kept(""), None);
	assert_eq!(bearer_from_kept(r#"{"access_token":""}"#), None);
}

#[test]
fn popup_ready_flattens_projects_and_status() {
	let ready = PopupReady {
		user: SessionUser {
			github_id: 1,
			github_login: "dev".into(),
			github_avatar_url: None,
			plan_tier: "free".into(),
		},
		organizations: vec![OrganizationSummary {
			login: "dev".into(),
			personal: true,
			avatar_url: None,
			supports_projects: true,
			repositories: Vec::new(),
			projects: vec![ProjectSummary {
				slug: "monochange".into(),
				name: "monochange".into(),
				description: String::new(),
				repository_count: 2,
			}],
		}],
		releases: vec![WebsiteRelease {
			version: "0.3.3".into(),
			summary: Vec::new(),
			sections: Vec::new(),
		}],
	};
	assert_eq!(ready.projects(), 1);
	assert_eq!(ready.status_line(), "v0.3.3 on monochange.dev");
	assert_eq!(
		ready.project_rows(),
		vec![ProjectRow {
			organization: "dev".into(),
			slug: "monochange".into(),
			name: "monochange".into(),
			repository_count: 2,
		}]
	);
	assert_eq!(ready.project_rows()[0].meta(), "dev/monochange · 2 repos");

	let wire = PopupWireState::from(PopupState::Ready(ready.clone()));
	assert_eq!(PopupState::from(wire), PopupState::Ready(ready));
}
