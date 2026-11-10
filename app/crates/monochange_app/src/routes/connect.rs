//! `/connect`

use leptos::prelude::Get;
use leptos::prelude::Resource;
use leptoskit::prelude::*;
use leptoskit_router::Route;

use crate::ui::heading;
use crate::ui::link;
use crate::ui::text;

/// Browser consent for a native or extension client.
#[derive(Route, Clone, Debug, PartialEq)]
pub struct Connect {
	pub client: Option<String>,
	pub redirect_uri: Option<String>,
	pub code_challenge: Option<String>,
	pub state: Option<String>,
	pub code_challenge_method: Option<String>,
}

pub fn page(route: &Connect) -> Element {
	let method = route.code_challenge_method.clone().unwrap_or_default();
	let client = route.client.clone().unwrap_or_default();
	let redirect = route.redirect_uri.clone().unwrap_or_default();
	let challenge = route.code_challenge.clone().unwrap_or_default();
	let state = route.state.clone().unwrap_or_default();
	let next = format!(
		"/connect?{}",
		url::form_urlencoded::Serializer::new(String::new())
			.append_pair("client", &client)
			.append_pair("redirect_uri", &redirect)
			.append_pair("code_challenge", &challenge)
			.append_pair("state", &state)
			.append_pair("code_challenge_method", &method)
			.finish()
	);
	let login = format!(
		"/login?{}",
		url::form_urlencoded::Serializer::new(String::new())
			.append_pair("next", &next)
			.finish()
	);
	let request = crate::slots::once(|| {
		Resource::new_blocking(
			move || {
				(
					client.clone(),
					redirect.clone(),
					challenge.clone(),
					state.clone(),
					method.clone(),
				)
			},
			|(client, redirect, challenge, state, method)| {
				async move {
					if method != "S256" {
						return Ok(None);
					}
					crate::server_fns::session::connect_request(client, redirect, challenge, state)
						.await
				}
			},
		)
	});

	let title = heading(1, "Connect monochange projects");
	let body = match request.get() {
		None => text("Checking connection…"),
		Some(Ok(Some(preview))) => {
			let who = text(format!("Continue as {}", preview.user.github_login));
			let copy = text(format!(
				"Allow the {} client to read your organizations and projects. You can revoke access in Account and devices.",
				preview.client
			));
			leptoskit::view! {
				<View class="flex flex-col gap-4">
					{who}
					{copy}
					{text("Approve")}
					{link(&preview.denial_url, "Deny")}
				</View>
			}
		}
		Some(Ok(None)) => {
			text("This connection request is invalid. Return to the app and try again.")
		}
		Some(Err(_)) => {
			let message = text("Sign in to review this connection request.");
			let action = link(&login, "Sign in with GitHub");
			leptoskit::view! {
				<View class="flex flex-col gap-4">
					{message}
					{action}
				</View>
			}
		}
	};
	leptoskit::view! {
		<View class="flex flex-col gap-4">
			{title}
			{body}
		</View>
	}
}
