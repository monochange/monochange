#![allow(clippy::disallowed_methods)]

use crate::OrganizationSummary;
use crate::ProjectSummary;
use crate::RepoInfo;
use crate::SessionUser;
use crate::WebsiteRelease;
use crate::feedback_console_path;
use crate::organization_path;
use crate::project_path;

#[test]
fn session_user_round_trips_json() {
	let user = SessionUser {
		github_id: 42,
		github_login: "monochange".into(),
		github_avatar_url: Some("https://example.test/a.png".into()),
		plan_tier: "free".into(),
	};
	let encoded = serde_json::to_string(&user).expect("encode");
	let decoded: SessionUser = serde_json::from_str(&encoded).expect("decode");
	assert_eq!(decoded.github_id, 42);
	assert_eq!(decoded.github_login, "monochange");
	assert_eq!(decoded.plan_tier, "free");
}

#[test]
fn organization_paths_match_dashboard_routes() {
	assert_eq!(organization_path("acme"), "/dashboard/acme");
	assert_eq!(
		project_path("acme", "core"),
		"/dashboard/acme/projects/core"
	);
	assert_eq!(
		feedback_console_path("acme", "core"),
		"/dashboard/acme/projects/core/feedback"
	);
}

#[test]
fn organization_summary_path_uses_login() {
	let summary = OrganizationSummary {
		login: "acme".into(),
		personal: false,
		avatar_url: None,
		supports_projects: true,
		repositories: vec![RepoInfo {
			id: 1,
			github_full_name: "acme/core".into(),
			github_private: false,
			plan_tier: "free".into(),
			installation_login: "acme".into(),
			installation_suspended: false,
		}],
		projects: vec![ProjectSummary {
			slug: "core".into(),
			name: "Core".into(),
			description: String::new(),
			repository_count: 1,
		}],
	};
	assert_eq!(summary.path(), "/dashboard/acme");
}

#[test]
fn website_release_round_trips_json() {
	let release = WebsiteRelease {
		version: "0.3.3".into(),
		summary: vec!["Ship notes".into()],
		sections: Vec::new(),
	};
	let encoded = serde_json::to_string(&release).expect("encode");
	let decoded: WebsiteRelease = serde_json::from_str(&encoded).expect("decode");
	assert_eq!(decoded.version, "0.3.3");
	assert_eq!(decoded.summary, vec!["Ship notes".to_string()]);
}
