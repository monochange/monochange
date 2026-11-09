//! Browser consent uses the shared authority's validated preview before showing actions.
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

#[component]
pub fn ConnectPage() -> impl IntoView {
	let query = use_query_map();
	let request = Resource::new_blocking(
		move || {
			let q = query.read();
			(
				q.get("client").unwrap_or_default(),
				q.get("redirect_uri").unwrap_or_default(),
				q.get("code_challenge").unwrap_or_default(),
				q.get("state").unwrap_or_default(),
				q.get("code_challenge_method").unwrap_or_default(),
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
	);
	let location = leptos_router::hooks::use_location();
	let next = move || format!("/connect?{}", location.search.get());
	let login = move || {
		format!(
			"/login?{}",
			url::form_urlencoded::Serializer::new(String::new())
				.append_pair("next", &next())
				.finish()
		)
	};
	view! { <section class="site-width dashboard-section"><h1>"Connect monochange projects"</h1>
	<Suspense fallback=|| view! { <p role="status">"Checking connection…"</p> }>
	{move || request.get().map(|result| match result {
		Ok(Some(preview)) => {
			let q = query.read();
			let redirect = q.get("redirect_uri").unwrap_or_default();
			let state = q.get("state").unwrap_or_default();
			let deny = preview.denial_url;
			view! { <div class="login-panel"><h2>{format!("Continue as {}", preview.user.github_login)}</h2><p>{format!("Allow the {} client to read your organizations and projects. You can revoke access in Account and devices.", preview.client)}</p>
				<form method="post" action="/connect/approve">
					<input type="hidden" name="client" value=q.get("client").unwrap_or_default() />
					<input type="hidden" name="redirect_uri" value=redirect />
					<input type="hidden" name="code_challenge" value=q.get("code_challenge").unwrap_or_default() />
					<input type="hidden" name="state" value=state />
					<button class="button button-brand" type="submit">"Approve"</button>
				</form><a class="button button-quiet" href=deny>"Deny"</a>
				<leptos::form::ActionForm action={ServerAction::<crate::server_fns::session::SwitchConnectAccount>::new()}><input type="hidden" name="next" value=next /><button type="submit">"Not you? Sign out"</button></leptos::form::ActionForm>
			</div> }.into_any()
		},
		Ok(None) => view! { <p role="alert">"This connection request is invalid. Return to the app and try again."</p> }.into_any(),
		Err(_) => view! { <p>"Sign in to review this connection request."</p><a class="button button-brand" href=login>"Sign in with GitHub"</a> }.into_any(),
	})}
	</Suspense></section> }
}
