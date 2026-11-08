//! Browser extension entry points: popup, options, and background worker.
//!
//! Surfaces talk to the app backend through [`monochange_extension_shared`]'s
//! HTTP RPC stub (swap to `leptoskit::rpc::client::call` when the web port
//! publishes the same operations). The bearer token lives in extension
//! storage under [`crate::session::SESSION_KEY`].
//!
//! UI trees use PascalCase `#[component]` functions and `view!` markup so
//! `leptoskit lint` (`component_attribute`, `markup_style`) stays green.
//! Styling is Tailwind utilities over `[theme]` `--mc-*` tokens (not semantic
//! CSS class sheets).

use leptoskit::extension::Background;
use leptoskit::extension::Surface;
use leptoskit::extension::runtime;
use leptoskit::extension::storage;
use leptoskit::extension::tabs;
use leptoskit::prelude::*;
use monochange_extension_shared::HttpRpcClient;
use monochange_extension_shared::RpcClient;
use monochange_extension_shared::RpcError;

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
	let tick = RwSignal::new(0_u32);
	let for_sign_in = route == "/sign-in";

	wasm_bindgen_futures::spawn_local(async move {
		let loaded = if for_sign_in {
			load_sign_in_state().await
		} else {
			match runtime::send::<ExtensionMessage, ExtensionReply>(&ExtensionMessage::Refresh)
				.await
			{
				Ok(ExtensionReply::State(wire)) => PopupState::from(wire),
				Ok(ExtensionReply::Failed(message)) => PopupState::Error { message },
				Ok(ExtensionReply::Done) => PopupState::Loading,
				Err(_) => {
					load_popup_state().await.unwrap_or_else(|error| {
						PopupState::Error {
							message: error.to_string(),
						}
					})
				}
			}
		};
		state.set(loaded);
		tick.set(tick.get().wrapping_add(1));
	});

	move || {
		tick.get();
		view! { <SurfaceRoot for_sign_in=for_sign_in state=state.get() /> }
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
				ExtensionMessage::SignedOut => {
					let _ = storage::LOCAL.remove(session::SESSION_KEY).await;
					ExtensionReply::Done
				}
			}
		}
	});
}

/// Picks the popup or options tree for the current glance state.
#[component]
fn SurfaceRoot(for_sign_in: bool, state: PopupState) -> Element {
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
			view! { <SignInReady login_url=login_url /> }
		}
		PopupState::SignedOut { login_url } => {
			view! {
				<Shell>
					<SignedOutBlock login_url=login_url />
				</Shell>
			}
		}
		PopupState::Ready(ready) if for_sign_in => {
			view! { <SignInConnected login=ready.user.github_login.clone() /> }
		}
		PopupState::Ready(ready) => {
			view! {
				<Shell>
					<ReadyBlock ready=ready />
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
			<Text class=chrome::EYEBROW>"Release floor"</Text>
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
fn SignInReady(#[prop(into)] login_url: String) -> Element {
	view! {
		<Shell>
			<View class=chrome::PANEL>
				<Text class=chrome::TITLE>"Sign in"</Text>
				<Text class=chrome::COPY>"Connect GitHub to glance at projects and releases."</Text>
				<OpenButton label="Sign in with GitHub" url=login_url quiet=false />
			</View>
		</Shell>
	}
}

/// Options page when a session is already present.
#[component]
fn SignInConnected(#[prop(into)] login: String) -> Element {
	view! {
		<Shell>
			<View class=chrome::PANEL>
				<Text class=chrome::TITLE>"Signed in"</Text>
				<Text class=chrome::COPY>{format!("Connected as @{login}")}</Text>
				<OpenButton
					label="Open workspace"
					url=format!("{DEFAULT_SERVER}/dashboard")
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
fn SignedOutBlock(#[prop(into)] login_url: String) -> Element {
	view! {
		<View class=chrome::PANEL>
			<Text class=chrome::TITLE>"Ship from one floor"</Text>
			<Text class=chrome::COPY>
				"Sign in to see projects and the latest release status on monochange.dev."
			</Text>
			<View class=chrome::ACTIONS>
				<OpenButton label="Sign in with GitHub" url=login_url quiet=false />
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
fn ReadyBlock(ready: PopupReady) -> Element {
	let login = ready.user.github_login.clone();
	let status = ready.status_line();
	let project_total = ready.projects();
	let rows = ready.project_rows();
	let project_list = if rows.is_empty() {
		view! {
			<Text class=chrome::COPY>
				"No projects yet. Open the dashboard to group repositories."
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
			{project_list}
			<View class=chrome::ACTIONS>
				<OpenButton
					label="Open workspace"
					url=format!("{DEFAULT_SERVER}/dashboard")
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
			<Text class="text-sm text-right text-[var(--mc-muted)]">{meta}</Text>
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

async fn load_sign_in_state() -> PopupState {
	match api_client().await.login_url(Some("/dashboard")).await {
		Ok(login_url) => PopupState::SignedOut { login_url },
		Err(error) => {
			PopupState::Error {
				message: error.to_string(),
			}
		}
	}
}

async fn load_popup_state() -> Result<PopupState, RpcError> {
	let client = api_client().await;
	let session = client.session().await?;
	let Some(user) = session else {
		let login_url = client
			.login_url(Some("/dashboard"))
			.await
			.unwrap_or_else(|_| format!("{DEFAULT_SERVER}/login"));
		return Ok(PopupState::SignedOut { login_url });
	};
	let organizations = client.organizations().await.unwrap_or_default();
	let releases = client.website_releases().await.unwrap_or_default();
	Ok(PopupState::Ready(PopupReady {
		user,
		organizations,
		releases,
	}))
}

async fn api_client() -> HttpRpcClient<ExtensionFetch> {
	let server = runtime::server().unwrap_or(DEFAULT_SERVER).to_owned();
	let mut client = HttpRpcClient::new(server, ExtensionFetch);
	let kept = storage::LOCAL
		.get::<String>(session::SESSION_KEY)
		.await
		.ok()
		.flatten();
	client.set_bearer(kept.as_deref().and_then(session::bearer_from_kept));
	client
}

/// Fetch transport for extension pages and the background worker.
///
/// The worker has no `window`, so this calls the global `fetch` the same way
/// leptoskit's commonplace extension transport does.
struct ExtensionFetch;

impl monochange_extension_shared::HttpTransport for ExtensionFetch {
	async fn send(
		&self,
		method: &str,
		url: &str,
		bearer: Option<&str>,
		body: Option<&[u8]>,
	) -> Result<monochange_extension_shared::HttpResponse, RpcError> {
		use js_sys::Reflect;
		use wasm_bindgen::JsCast;
		use wasm_bindgen::JsValue;
		use wasm_bindgen_futures::JsFuture;

		let failed = |_| RpcError::Unreachable("monochange couldn't reach its server".into());
		let init = web_sys::RequestInit::new();
		init.set_method(method);
		init.set_credentials(web_sys::RequestCredentials::Omit);
		let headers = web_sys::Headers::new().map_err(failed)?;
		headers.set("accept", "application/json").map_err(failed)?;
		if let Some(token) = bearer {
			headers
				.set("authorization", &format!("Bearer {token}"))
				.map_err(failed)?;
		}
		if let Some(bytes) = body {
			headers
				.set("content-type", "application/json")
				.map_err(failed)?;
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
		Ok(monochange_extension_shared::HttpResponse {
			status,
			body: js_sys::Uint8Array::new(&buffer).to_vec(),
		})
	}
}
