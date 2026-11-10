//! Browser-owned device session management.
use leptos::prelude::*;

#[component]
pub fn AccountPage() -> impl IntoView {
	let revoke = ServerAction::<crate::server_fns::session::RevokeClientSession>::new();
	let everywhere = ServerAction::<crate::server_fns::session::SignOutEverywhere>::new();
	let sessions = Resource::new_blocking(
		move || (revoke.version().get(), everywhere.version().get()),
		|_| crate::server_fns::session::list_client_sessions(),
	);
	view! {
		<section class="site-width dashboard-section">
			<h1>"Account and devices"</h1>
			<Suspense fallback=|| view! { <p role="status">"Loading devices…"</p> }>
				{move || {
					sessions.get().map(|result| {
						match result {
							Ok(sessions) if sessions.is_empty() => {
								view! { <p>"No connected devices."</p> }.into_any()
							}
							Ok(sessions) => {
								view! {
									<ul class="repository-list">
										{sessions
											.into_iter()
											.map(|session| {
												view! {
													<li>
														<p>
															{format!(
																"{} · expires {}",
																session.client, session.expires_at
															)}
														</p>
														<leptos::form::ActionForm action=revoke>
															<input
																type="hidden"
																name="id"
																value=session.id
															/>
															<button
																type="submit"
																class="button button-quiet"
															>
																"Revoke"
															</button>
														</leptos::form::ActionForm>
													</li>
												}
											})
											.collect::<Vec<_>>()}
									</ul>
								}
								.into_any()
							}
							Err(_) => {
								view! {
									<p role="alert">
										"Your devices couldn't be loaded. Sign in or reload to try again."
									</p>
									<a href="/login?next=%2Faccount">"Sign in"</a>
								}
								.into_any()
							}
						}
					})
				}}
			</Suspense>
			{move || {
				revoke.value().get().and_then(Result::err).map(
					|_| view! { <p role="alert">"The device couldn't be revoked. Try again."</p> },
				)
			}}
			{move || {
				everywhere.value().get().map(|result| {
					match result {
						Ok(()) => {
							view! { <leptos_router::components::Redirect path="/login" /> }
								.into_any()
						}
						Err(_) => {
							view! { <p role="alert">"Sign out failed. Please try again."</p> }
								.into_any()
						}
					}
				})
			}}
			<leptos::form::ActionForm action=everywhere>
				<button type="submit" class="button button-brand">"Sign out everywhere"</button>
			</leptos::form::ActionForm>
		</section>
	}
}
