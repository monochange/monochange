use super::*;
fn sample_orgs() -> Vec<OrganizationSummary> {
	vec![
		OrganizationSummary {
			login: "ifiokjr".into(),
			personal: true,
			avatar_url: None,
			supports_projects: true,
			repositories: Vec::new(),
			projects: vec![
				ProjectSummary {
					slug: "monochange".into(),
					name: "monochange".into(),
					description: "Release planning".into(),
					repository_count: 1,
				},
				ProjectSummary {
					slug: "other".into(),
					name: "other".into(),
					description: String::new(),
					repository_count: 0,
				},
			],
		},
		OrganizationSummary {
			login: "acme".into(),
			personal: false,
			avatar_url: None,
			supports_projects: true,
			repositories: Vec::new(),
			projects: vec![ProjectSummary {
				slug: "sdk".into(),
				name: "SDK".into(),
				description: String::new(),
				repository_count: 3,
			}],
		},
	]
}

fn sample_releases() -> Vec<WebsiteRelease> {
	vec![
		WebsiteRelease {
			version: "0.3.3".into(),
			summary: vec!["Feedback SDKs".into()],
			sections: vec![WebsiteSection {
				title: "Added".into(),
				entries: Vec::new(),
			}],
		},
		WebsiteRelease {
			version: "0.3.2".into(),
			summary: Vec::new(),
			sections: Vec::new(),
		},
	]
}

#[test]
fn project_and_release_glance_describes_server_data() {
	assert_eq!(project_count(&sample_orgs()), 3);
	assert_eq!(
		release_status_label(&sample_releases()),
		"v0.3.3 on monochange.dev"
	);
	assert_eq!(project_count(&[]), 0);
	assert_eq!(release_status_label(&[]), "No published website releases");
}
