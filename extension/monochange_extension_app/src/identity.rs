//! The worker owns the complete interactive handoff, including storage.
use monochange_shared::ClientKind;
use monochange_shared::ClientSession;
use monochange_shared::api::ApiClient;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "
export function redirectURL() { return (globalThis.browser || globalThis.chrome).identity.getRedirectURL(); }
export function entropy() { return crypto.getRandomValues(new Uint8Array(32)); }
export async function authenticate(url) {
 if (globalThis.browser) return await browser.identity.launchWebAuthFlow({url, interactive:true});
 return await new Promise((resolve,reject) => chrome.identity.launchWebAuthFlow({url,interactive:true}, result => {
  const error=chrome.runtime.lastError;
  if(error) reject(new Error(error.message)); else if(!result) reject(new Error('Sign-in cancelled')); else resolve(result);
 }));
}
")]
extern "C" {
	#[wasm_bindgen(catch, js_name = redirectURL)]
	fn redirect_url() -> Result<String, JsValue>;
	#[wasm_bindgen(catch)]
	fn entropy() -> Result<js_sys::Uint8Array, JsValue>;
	#[wasm_bindgen(catch)]
	async fn authenticate(url: &str) -> Result<JsValue, JsValue>;
}

thread_local! { static SIGNING_IN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
struct SignInGuard;
impl Drop for SignInGuard {
	fn drop(&mut self) {
		SIGNING_IN.set(false);
	}
}
pub async fn sign_in() -> Result<(), String> {
	if SIGNING_IN.replace(true) {
		return Err("Sign-in is already in progress".into());
	}
	let _guard = SignInGuard;
	let redirect = redirect_url().map_err(|_| "Browser identity is unavailable")?;
	let random: [u8; 32] = entropy()
		.map_err(|_| "Secure randomness is unavailable")?
		.to_vec()
		.try_into()
		.map_err(|_| "Invalid entropy")?;
	let (verifier, challenge) = monochange_shared::pkce_pair(random);
	let random: [u8; 32] = entropy()
		.map_err(|_| "Secure randomness is unavailable")?
		.to_vec()
		.try_into()
		.map_err(|_| "Invalid entropy")?;
	let (state, _) = monochange_shared::pkce_pair(random);
	let api = ApiClient::new(
		crate::extension::ExtensionFetch,
		leptoskit::extension::runtime::server().unwrap_or(crate::DEFAULT_SERVER),
	);
	let connect = api.connect_url(ClientKind::Extension, &redirect, &challenge, &state);
	let callback = authenticate(&connect)
		.await
		.map_err(|_| "Browser sign-in was cancelled or unavailable")?
		.as_string()
		.ok_or("Missing browser callback")?;
	let request = monochange_shared::client_auth::ClientHandoff {
		client: ClientKind::Extension,
		redirect_uri: redirect,
		state,
		verifier,
	}
	.consume(&callback)
	.map_err(str::to_owned)?;
	let saved: ClientSession = api.exchange(request).await.map_err(|e| e.to_string())?;
	if let Err(error) = leptoskit::extension::storage::LOCAL
		.set(crate::session::SESSION_KEY, &saved)
		.await
	{
		let _ = api.revoke(&saved.access_token).await;
		return Err(format!("Your session couldn't be stored: {error}"));
	}
	Ok(())
}
