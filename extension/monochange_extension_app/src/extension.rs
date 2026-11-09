use leptoskit::extension::Background;
use leptoskit::extension::Surface;
use leptoskit::extension::runtime;
use leptoskit::extension::storage;
use leptoskit::extension::tabs;
use leptoskit::prelude::*;
use monochange_shared::api::ApiClient;
use monochange_shared::api::ApiError as RpcError;
use monochange_shared::api::HttpResponse;
use monochange_shared::api::Transport;

use crate::DEFAULT_SERVER;
use crate::chrome;
use crate::popup::ExtensionMessage;
use crate::popup::ExtensionReply;
use crate::popup::PopupReady;
use crate::popup::PopupState;
use crate::popup::PopupWireState;
use crate::popup::ProjectRow;
use crate::session;

/// Opens a surface at its configured route (`/` popup, `/sign-in` options).
pub fn main(surface: Surface) -> impl Fn() -> Element + Send + Sync + 'static {
	let (colors, lengths) = chrome::theme_maps();
	let _ = Theme::install_maps(&colors, &lengths);

	let route = surface.route().to_owned();
	let state = RwSignal::new(PopupState::Loading);
	let for_sign_in = route == "/sign-in";

	wasm_bindgen_futures::spawn_local(async move {
		let loaded =
			match runtime::send::<ExtensionMessage, ExtensionReply>(&ExtensionMessage::Refresh)
				.await
			{
				Ok(ExtensionReply::State(wire)) => PopupState::from(wire),
				Ok(ExtensionReply::Failed(message)) => PopupState::Error { message },
				Ok(ExtensionReply::Done) => {
					PopupState::Error {
						message:
							"Your session couldn't be loaded. Reload the extension to try again."
								.into(),
					}
				}
				Err(_) => PopupState::Error {
					message:
						"The extension background worker couldn't be reached. Reload to try again."
							.into(),
				},
			};
		state.set(loaded);
	});

	move || {
		view! { <SurfaceRoot for_sign_in=for_sign_in state=state.get() controller=state /> }
	}
}

/// Registers worker handlers for glance refresh and session changes.
pub fn background(worker: &Background) {
	worker.on_installed(|_| {
		async {
			let _ = ();
		}
	});
	worker.on_message(|message: ExtensionMessage, _sender| {
		async move {
			match message {
				ExtensionMessage::Refresh => {
					match load_popup_state().await {
						Ok(state) => ExtensionReply::State(PopupWireState::from(state)),
						Err(error) => ExtensionReply::Failed(error.to_string()),
					}
				}
				ExtensionMessage::SignIn => {
					match crate::identity::sign_in().await {
						Ok(()) => {
							ExtensionReply::State(PopupWireState::from(
								load_popup_state().await.unwrap_or_else(|e| {
									PopupState::Error {
										message: e.to_string(),
									}
								}),
							))
						}
						Err(error) => ExtensionReply::Failed(error),
					}
				}
				ExtensionMessage::SignedOut => {
					let result = async {
						let client = api_client().await.map_err(|e| e.to_string())?;
						let revoke = if let Some(token) = client.bearer() {
							client.revoke(token).await.map_err(|e| e.to_string())
						} else {
							Ok(())
						};
						storage::LOCAL
							.remove(session::SESSION_KEY)
							.await
							.map_err(|e| e.to_string())?;
						revoke
					}
					.await;
					match result {
						Ok(()) => ExtensionReply::Done,
						Err(error) => ExtensionReply::Failed(error),
					}
				}
			}
		}
	});
}

/// Picks the popup or options tree for the current glance state.
#[component]
fn SurfaceRoot(for_sign_in: bool, state: PopupState, controller: RwSignal<PopupState>) -> Element {
	match state {
		PopupState::Loading if for_sign_in => view! { <SignInLoading /> },
		PopupState::Loading => {
			view! {
				<Shell>
					<StatusBlock message="Loading your workspace…" />
				</Shell>
			}
		}
		PopupState::SignedOut { login_url } if for_sign_in => {
			view! { <SignInReady login_url=login_url controller=controller /> }
		}
		PopupState::SignedOut { login_url } => {
			view! {
				<Shell>
					<SignedOutBlock login_url=login_url controller=controller />
				</Shell>
			}
		}
		PopupState::Ready(ready) if for_sign_in => {
			view! { <SignInConnected login=ready.user.github_login.clone() controller=controller /> }
		}
		PopupState::Ready(ready) => {
			view! {
				<Shell>
					<ReadyBlock ready=ready controller=controller />
				</Shell>
			}
		}
		PopupState::Error { message } => {
			view! {
				<Shell>
					<StatusBlock message=message />
				</Shell>
			}
		}
	}
}

/// Brand chrome around a popup body.
#[component]
fn Shell(children: Children) -> Element {
	view! {
		<View class=chrome::SHELL>
			<ChromeHeader />
			{children}
		</View>
	}
}

/// Compact brand row: mark name + Release floor label.
#[component]
fn ChromeHeader() -> Element {
	view! {
		<View class=chrome::CHROME>
			<Button
				class=chrome::BRAND
				on:press=|| {
					wasm_bindgen_futures::spawn_local(async {
						let _ = tabs::open("https://monochange.dev").await;
					});
				}
			>
				"monochange"
			</Button>
			<Text class=chrome::EYEBROW>"projects"</Text>
		</View>
	}
}

/// Options page while the login URL is loading.
#[component]
fn SignInLoading() -> Element {
	view! {
		<Shell>
			<View class=chrome::PANEL>
				<Text class=chrome::TITLE>"Sign in"</Text>
				<Text class=chrome::STATUS>"Preparing GitHub sign-in…"</Text>
			</View>
		</Shell>
	}
}

/// Options page with a GitHub sign-in action.
#[component]
fn SignInReady(#[prop(into)] login_url: String, controller: RwSignal<PopupState>) -> Element {
	let _ = login_url;

	view! {
		<Shell>
			<View class=chrome::PANEL>
				<Text class=chrome::TITLE>"Sign in"</Text>
				<Text class=chrome::COPY>"Connect GitHub to glance at projects and releases."</Text>
				<SessionButton sign_in=true controller=controller />
			</View>
		</Shell>
	}
}

/// Options page when a session is already present.
#[component]
fn SignInConnected(#[prop(into)] login: String, controller: RwSignal<PopupState>) -> Element {
	view! {
		<Shell>
			<View class=chrome::PANEL>
				<Text class=chrome::TITLE>"Signed in"</Text>
				<Text class=chrome::COPY>{format!("Connected as @{login}")}</Text><SessionButton sign_in=false controller=controller />
				<OpenButton
					label="Open projects"
					url=format!("{}/projects", runtime::server().unwrap_or(DEFAULT_SERVER))
					quiet=false
				/>
			</View>
		</Shell>
	}
}

/// Quiet status panel.
#[component]
fn StatusBlock(#[prop(into)] message: String) -> Element {
	view! {
		<View class=chrome::PANEL>
			<Text class=chrome::STATUS>{message}</Text>
		</View>
	}
}

/// Signed-out popup body with sign-in and options actions.
#[component]
fn SignedOutBlock(#[prop(into)] login_url: String, controller: RwSignal<PopupState>) -> Element {
	let _ = login_url;
	view! {
		<View class=chrome::PANEL>
			<Text class=chrome::TITLE>"Ship from one floor"</Text>
			<Text class=chrome::COPY>
				"Sign in to see projects and the latest release status on monochange.dev."
			</Text>
			<View class=chrome::ACTIONS>
				<SessionButton sign_in=true controller=controller />
				<Button
					class=chrome::BUTTON_QUIET
					on:press=|| {
						wasm_bindgen_futures::spawn_local(async {
							let _ = runtime::open_options().await;
						});
					}
				>
					"Options"
				</Button>
			</View>
		</View>
	}
}

/// Signed-in glance: release status and project rows.
#[component]
fn ReadyBlock(ready: PopupReady, controller: RwSignal<PopupState>) -> Element {
	let login = ready.user.github_login.clone();
	let status = ready.status_line();
	let project_total = ready.projects();
	let rows = ready.project_rows();
	let project_list = if rows.is_empty() {
		view! {
			<Text class=chrome::COPY>
				"No projects yet. Open projects to group repositories."
			</Text>
		}
	} else {
		view! {
			<View class="gap-0">
				{rows
					.into_iter()
					.take(6)
					.map(|row| view! { <ProjectRowView row=row /> })
					.collect::<Vec<_>>()}
			</View>
		}
	};

	view! {
		<View class=chrome::PANEL>
			<Text class=chrome::STATUS_LIVE>{format!("@{login} · {status}")}</Text>
			<Text class=chrome::EYEBROW>{format!("{project_total} projects")}</Text>
			{project_list}<SessionButton sign_in=false controller=controller />
			<View class=chrome::ACTIONS>
				<OpenButton
					label="Open projects"
					url=format!("{}/projects", runtime::server().unwrap_or(DEFAULT_SERVER))
					quiet=false
				/>
			</View>
		</View>
	}
}

/// One project line in the glance list.
#[component]
fn ProjectRowView(row: ProjectRow) -> Element {
	let title = row.name.clone();
	let meta = row.meta();
	view! {
		<View class=chrome::ROW>
			<Text class="font-bold text-[var(--mc-ink)]">{title}</Text>
			<Text class="text-sm text-[var(--mc-muted)]">{meta}</Text>
		</View>
	}
}

/// Opens a URL in a new tab when pressed.
#[component]
fn OpenButton(label: &'static str, #[prop(into)] url: String, quiet: bool) -> Element {
	let class = if quiet {
		chrome::BUTTON_QUIET
	} else {
		chrome::BUTTON
	};
	view! {
		<Button
			class=class
			on:press=move || {
				let url = url.clone();
				wasm_bindgen_futures::spawn_local(async move {
					let _ = tabs::open(&url).await;
				});
			}
		>
			{label}
		</Button>
	}
}

async fn api_client() -> Result<ApiClient<ExtensionFetch>, RpcError> {
	let server = runtime::server().unwrap_or(DEFAULT_SERVER);
	let saved = storage::LOCAL
		.get::<monochange_shared::ClientSession>(session::SESSION_KEY)
		.await
		.map_err(|e| RpcError::Transport(e.to_string()))?;
	Ok(match saved {
		Some(saved) => ApiClient::with_bearer(ExtensionFetch, server, saved.access_token),
		None => ApiClient::new(ExtensionFetch, server),
	})
}
async fn load_popup_state() -> Result<PopupState, RpcError> {
	match load_popup_state_inner().await {
		Err(RpcError::Status { status: 401, .. }) => {
			storage::LOCAL
				.remove(session::SESSION_KEY)
				.await
				.map_err(|e| RpcError::Transport(e.to_string()))?;
			Ok(PopupState::SignedOut {
				login_url: String::new(),
			})
		}
		result => result,
	}
}
async fn load_popup_state_inner() -> Result<PopupState, RpcError> {
	let client = api_client().await?;
	if client.bearer().is_none() {
		return Ok(PopupState::SignedOut {
			login_url: String::new(),
		});
	}
	let user = match client.session_user().await {
		Ok(Some(user)) => user,
		Ok(None) | Err(RpcError::Status { status: 401, .. }) => {
			storage::LOCAL
				.remove(session::SESSION_KEY)
				.await
				.map_err(|e| RpcError::Transport(e.to_string()))?;
			return Ok(PopupState::SignedOut {
				login_url: String::new(),
			});
		}
		Err(error) => return Err(error),
	};
	let organizations = client.organizations().await?;
	let releases = client.website_releases().await?;
	Ok(PopupState::Ready(PopupReady {
		user,
		organizations,
		releases,
	}))
}
/// Fetch transport for extension pages and the background worker.
///
/// The worker has no `window`, so this calls the global `fetch` the same way
/// leptoskit's commonplace extension transport does.
pub(crate) struct ExtensionFetch;

impl Transport for ExtensionFetch {
	async fn post(
		&self,
		url: &str,
		bearer: Option<&str>,
		content_type: &str,
		body: &str,
	) -> Result<HttpResponse, RpcError> {
		use js_sys::Reflect;
		use wasm_bindgen::JsCast;
		use wasm_bindgen::JsValue;
		use wasm_bindgen_futures::JsFuture;

		let failed = |_| RpcError::Transport("monochange couldn't reach its server".into());
		let init = web_sys::RequestInit::new();
		init.set_method("POST");
		init.set_credentials(web_sys::RequestCredentials::Omit);
		let headers = web_sys::Headers::new().map_err(failed)?;
		headers.set("accept", "application/json").map_err(failed)?;
		if let Some(token) = bearer {
			headers
				.set("authorization", &format!("Bearer {token}"))
				.map_err(failed)?;
		}
		{
			let bytes = body.as_bytes();
			headers.set("content-type", content_type).map_err(failed)?;
			init.set_body(&js_sys::Uint8Array::from(bytes).into());
		}
		init.set_headers(&headers.into());
		let global = js_sys::global();
		let fetch: js_sys::Function = Reflect::get(&global, &JsValue::from_str("fetch"))
			.map_err(failed)?
			.dyn_into()
			.map_err(failed)?;
		let promise: js_sys::Promise = fetch
			.call2(&global, &JsValue::from_str(url), &init)
			.map_err(failed)?
			.dyn_into()
			.map_err(failed)?;
		let response: web_sys::Response = JsFuture::from(promise)
			.await
			.map_err(failed)?
			.dyn_into()
			.map_err(failed)?;
		let status = response.status();
		let buffer = JsFuture::from(response.array_buffer().map_err(failed)?)
			.await
			.map_err(failed)?;
		Ok(HttpResponse {
			status,
			body: String::from_utf8(js_sys::Uint8Array::new(&buffer).to_vec())
				.map_err(|e| RpcError::Decode(e.to_string()))?,
		})
	}
}

#[component]
fn SessionButton(sign_in: bool, controller: RwSignal<PopupState>) -> Element {
	let status = RwSignal::new(String::new());
	view! { <View><Button class=chrome::BUTTON on:press=move || {
		 status.set("Working…".into());
		 wasm_bindgen_futures::spawn_local(async move {
		  let message = if sign_in { ExtensionMessage::SignIn } else { ExtensionMessage::SignedOut };
		  match runtime::send::<ExtensionMessage,ExtensionReply>(&message).await {
		   Ok(ExtensionReply::Failed(error)) => status.set(error),
		   Ok(ExtensionReply::State(wire)) => controller.set(PopupState::from(wire)),
	Ok(ExtensionReply::Done) => controller.set(PopupState::SignedOut { login_url: String::new() }),
		   Err(error) => status.set(error.to_string()),
		  }
		 });
		}>{if sign_in { "Sign in with GitHub" } else { "Sign out" }}</Button><Text>{move || status.get()}</Text></View> }
}

#[cfg(test)]
#[path = "__tests__/extension_tests.rs"]
mod tests;
