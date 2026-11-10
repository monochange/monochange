use std::sync::Arc;
use std::sync::Mutex;

use leptos::prelude::*;
use leptos_router::components::Route;
use leptos_router::components::Router;
use leptos_router::components::Routes;
use leptos_router::components::provide_server_redirect;
use leptos_router::location::RequestUrl;
use leptos_router::path;
use rstest::rstest;

use super::SessionActionFeedback;
use crate::server_fns::session::RevokeClientSession;
use crate::server_fns::session::SignOutEverywhere;

#[rstest]
#[case::revoke_failure("revoke_failure", true, None)]
#[case::sign_out_failure("sign_out_failure", false, Some(false))]
#[case::signed_out("signed_out", false, Some(true))]
fn session_action_feedback_shows_the_outcome(
	#[case] scenario: &str,
	#[case] revoke_failed: bool,
	#[case] signed_out: Option<bool>,
) {
	let destination = Arc::new(Mutex::new(None));
	let output = Owner::new().with(|| {
		provide_context(RequestUrl::new("http://localhost/"));
		let redirect = destination.clone();
		provide_server_redirect(move |path, permanent| {
			*redirect.lock().unwrap() = Some((path.to_owned(), permanent));
		});
		let revoke = ServerAction::<RevokeClientSession>::new();
		let everywhere = ServerAction::<SignOutEverywhere>::new();
		if revoke_failed {
			revoke
				.value()
				.set(Some(Err(ServerFnError::new("request failed"))));
		}
		if let Some(success) = signed_out {
			everywhere.value().set(Some(if success {
				Ok(())
			} else {
				Err(ServerFnError::new("request failed"))
			}));
		}
		view! {
			<Router>
				<Routes fallback=|| ()>
					<Route
						path=path!("/")
						view=move || view! { <SessionActionFeedback revoke everywhere /> }
					/>
				</Routes>
			</Router>
		}
		.to_html()
	});
	insta::with_settings!({ snapshot_suffix => scenario }, {
		insta::assert_snapshot!(output);
	});
	assert_eq!(
		*destination.lock().unwrap(),
		(signed_out == Some(true)).then(|| ("/login".to_owned(), false)),
	);
}
