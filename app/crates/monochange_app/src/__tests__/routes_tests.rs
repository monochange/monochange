// Tokio's test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use leptoskit_router::Render;
use leptoskit_router::Routable;

use super::AuthCallback;
use super::BookChapter;
use super::Connect;
use super::FeedbackConsolePage;
use super::Navigation;
use super::OrganizationPage;
use super::PortalPage;
use super::ProjectDetailsPage;
use super::ProjectFeedback;
use super::ProjectOrganization;
use super::ProjectPage;
use super::Route;
use super::SignIn;
use super::Workspace;

fn connect(method: &str) -> Connect {
	Connect {
		client: Some("native".to_owned()),
		redirect_uri: Some("dev.monochange.app:/auth/native/return".to_owned()),
		code_challenge: Some("challenge".to_owned()),
		state: Some("state-1234567890".to_owned()),
		code_challenge_method: Some(method.to_owned()),
	}
}

#[test]
fn every_route_value_converts_into_the_router_enum() {
	let routes = [
		Route::from(BookChapter {
			chapter: vec!["guide".to_owned()],
		}),
		Route::from(connect("S256")),
		Route::from(Workspace {
			q: Some("web".to_owned()),
			org: None,
		}),
		Route::from(SignIn {
			next: Some("/dashboard".to_owned()),
		}),
		Route::from(AuthCallback {
			code: Some("code".to_owned()),
			state: Some("state".to_owned()),
		}),
		Route::from(OrganizationPage {
			organization: "acme".to_owned(),
			q: Some("api".to_owned()),
		}),
		Route::from(ProjectOrganization {
			organization: "acme".to_owned(),
			q: None,
		}),
		Route::from(ProjectPage {
			organization: "acme".to_owned(),
			project: "widget".to_owned(),
		}),
		Route::from(ProjectDetailsPage {
			organization: "acme".to_owned(),
			project: "widget".to_owned(),
		}),
		Route::from(FeedbackConsolePage {
			organization: "acme".to_owned(),
			project: "widget".to_owned(),
		}),
		Route::from(ProjectFeedback {
			organization: "acme".to_owned(),
			project: "widget".to_owned(),
		}),
		Route::from(PortalPage {
			organization: "acme".to_owned(),
			project: "widget".to_owned(),
			shared: None,
		}),
	];

	for route in routes {
		let href = route.href();
		let parsed = Route::parse(href.as_str()).unwrap_or_else(|_| {
			panic!("{} should parse", href.as_str());
		});
		assert_eq!(parsed, route);
	}
}

#[test]
fn dashboard_aliases_parse_their_parameters() {
	for href in [
		"/dashboard/acme",
		"/dashboard/acme?q=web",
		"/dashboard/acme/projects/widget",
		"/dashboard/acme/projects/widget/feedback",
		"/connect?client=native&code_challenge_method=plain",
	] {
		let route = Route::parse(href).unwrap_or_else(|_| panic!("{href} should parse"));
		assert!(!route.href().as_str().is_empty());
	}
}

#[tokio::test]
async fn nested_dashboard_routes_and_connect_render() {
	let samples = [
		Route::Connect(connect("plain")),
		Route::Connect(connect("S256")),
		Route::OrganizationPage(OrganizationPage {
			organization: "acme".to_owned(),
			q: None,
		}),
		Route::ProjectPage(ProjectPage {
			organization: "acme".to_owned(),
			project: "widget".to_owned(),
		}),
		Route::FeedbackConsolePage(FeedbackConsolePage {
			organization: "acme".to_owned(),
			project: "widget".to_owned(),
		}),
	];

	let state = crate::tests::memory_state().await;
	for route in samples {
		let rendered = route.clone();
		let state = state.clone();
		let html = crate::tests::render(move || {
			leptos::prelude::provide_context(state.clone());
			let navigation = Navigation::new(rendered.clone());
			leptoskit::web::render(rendered.render(&navigation))
		})
		.await;
		assert!(html.contains("<body"), "{html}");
	}
}
