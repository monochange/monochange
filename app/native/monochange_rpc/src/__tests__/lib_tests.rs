use super::AuthHandshake;
use super::ProjectSummary;
use super::SessionUser;

#[test]
fn anonymous_handshake_uses_stable_protocol() {
	let handshake = AuthHandshake::anonymous();
	assert_eq!(handshake.protocol, AuthHandshake::PROTOCOL);
	assert_eq!(handshake.status, "anonymous");
	assert!(handshake.session.is_none());
}

#[test]
fn signed_in_handshake_keeps_session() {
	let session = SessionUser {
		github_id: 42,
		github_login: "octocat".to_owned(),
		github_avatar_url: Some("https://example.test/a.png".to_owned()),
		plan_tier: "pro".to_owned(),
	};
	let handshake = AuthHandshake::signed_in(session.clone());
	assert_eq!(handshake.status, "signed_in");
	assert_eq!(handshake.session.as_ref(), Some(&session));
}

#[test]
fn project_summary_round_trips_json() {
	let project = ProjectSummary {
		organization: "acme".to_owned(),
		slug: "billing".to_owned(),
		name: "Billing".to_owned(),
		description: "Invoices".to_owned(),
		repository_count: 3,
	};
	let encoded = serde_json::to_string(&project).expect("serialize");
	let decoded: ProjectSummary = serde_json::from_str(&encoded).expect("deserialize");
	assert_eq!(decoded, project);
}
