//! Browser extension entry points: popup, options, and background worker.
//!
//! Surfaces talk to the app backend through [`monochange_extension_shared`]'s
//! HTTP RPC stub (swap to `leptoskit::rpc::client::call` when the web port
//! publishes the same operations). The bearer token lives in extension
//! storage under [`crate::session::SESSION_KEY`].

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
use crate::popup::ExtensionMessage;
use crate::popup::ExtensionReply;
use crate::popup::PopupReady;
use crate::popup::PopupState;
use crate::popup::PopupWireState;
use crate::session;

/// Opens a surface at its configured route (`/` popup, `/sign-in` options).
pub fn main(surface: Surface) -> impl Fn() -> Element + Send + Sync + 'static {
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
		match state.get() {
			PopupState::Loading if for_sign_in => sign_in_loading(),
			PopupState::Loading => shell(status_block("Loading your workspace…")),
			PopupState::SignedOut { login_url } if for_sign_in => sign_in_ready(login_url),
			PopupState::SignedOut { login_url } => shell(signed_out_block(login_url)),
			PopupState::Ready(ready) if for_sign_in => {
				shell(
					View()
						.class("mc-panel")
						.child(Text("Signed in").class("mc-title"))
						.child(
							Text(format!("Connected as @{}", ready.user.github_login))
								.class("mc-copy"),
						)
						.child(open_button(
							"Open workspace",
							format!("{DEFAULT_SERVER}/dashboard"),
							"mc-button",
						)),
				)
			}
			PopupState::Ready(ready) => shell(ready_block(ready)),
			PopupState::Error { message } => shell(status_block(message)),
		}
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

fn shell(body: Element) -> Element {
	View().class("mc-shell").child(chrome_header()).child(body)
}

fn sign_in_loading() -> Element {
	shell(
		View()
			.class("mc-panel")
			.child(Text("Sign in").class("mc-title"))
			.child(Text("Preparing GitHub sign-in…").class("mc-status")),
	)
}

fn sign_in_ready(login_url: String) -> Element {
	shell(
		View()
			.class("mc-panel")
			.child(Text("Sign in").class("mc-title"))
			.child(Text("Connect GitHub to glance at projects and releases.").class("mc-copy"))
			.child(open_button("Sign in with GitHub", login_url, "mc-button")),
	)
}

fn chrome_header() -> Element {
	View()
		.class("mc-chrome")
		.child(
			Button("monochange")
				.class("mc-brand mc-brand-name")
				.on_press(|| {
					wasm_bindgen_futures::spawn_local(async {
						let _ = tabs::open("https://monochange.dev").await;
					});
				}),
		)
		.child(Text("Release floor").class("mc-eyebrow"))
}

fn status_block(message: impl Into<String>) -> Element {
	View()
		.class("mc-panel")
		.child(Text(message.into()).class("mc-status"))
}

fn signed_out_block(login_url: String) -> Element {
	View()
		.class("mc-panel")
		.child(Text("Ship from one floor").class("mc-title"))
		.child(
			Text("Sign in to see projects and the latest release status on monochange.dev.")
				.class("mc-copy"),
		)
		.child(
			View()
				.class("mc-actions")
				.child(open_button("Sign in with GitHub", login_url, "mc-button"))
				.child(
					Button("Options")
						.class("mc-button mc-button-quiet")
						.on_press(|| {
							wasm_bindgen_futures::spawn_local(async {
								let _ = runtime::open_options().await;
							});
						}),
				),
		)
}

fn ready_block(ready: PopupReady) -> Element {
	let login = ready.user.github_login.clone();
	let status = ready.status_line();
	let project_total = ready.projects();
	let rows = ready.project_rows();

	let mut list = View()
		.class("mc-panel")
		.child(Text(format!("@{login} · {status}")).class("mc-status"));
	list = list.child(Text(format!("{project_total} projects")).class("mc-eyebrow"));

	if rows.is_empty() {
		list = list.child(
			Text("No projects yet. Open the dashboard to group repositories.").class("mc-copy"),
		);
	} else {
		for row in rows.into_iter().take(6) {
			let title = row.name.clone();
			let meta = row.meta();
			list = list.child(
				View()
					.class("mc-row")
					.child(Text(title).class("mc-row-title"))
					.child(Text(meta).class("mc-row-meta")),
			);
		}
	}

	list.child(View().class("mc-actions").child(open_button(
		"Open workspace",
		format!("{DEFAULT_SERVER}/dashboard"),
		"mc-button",
	)))
}

fn open_button(label: &'static str, url: String, class: &'static str) -> Element {
	Button(label).class(class).on_press(move || {
		let url = url.clone();
		wasm_bindgen_futures::spawn_local(async move {
			let _ = tabs::open(&url).await;
		});
	})
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
