//! Native credential lifecycle. Browser approval and device storage stay separate.
use monochange_shared::ClientSession;
use monochange_shared::api::ApiClient;
use monochange_shared::api::ApiError;
use monochange_shared::api::HttpResponse;
use monochange_shared::api::Transport;

pub struct NativeTransport;
impl Transport for NativeTransport {
	async fn post(
		&self,
		url: &str,
		bearer: Option<&str>,
		content_type: &str,
		body: &str,
	) -> Result<HttpResponse, ApiError> {
		post(url, bearer, content_type, body).await
	}
}
pub fn client(session: Option<&ClientSession>) -> ApiClient<NativeTransport> {
	match session {
		Some(session) => {
			ApiClient::with_bearer(NativeTransport, crate::SERVER, &session.access_token)
		}
		None => ApiClient::new(NativeTransport, crate::SERVER),
	}
}

#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
mod native {
	use leptoskit_plugin_secure_store::Key;
	use leptoskit_plugin_secure_store::guest::SecureStore;
	use leptoskit_plugin_web_auth::guest::WebAuth;

	use super::*;
	fn storage_key() -> Result<Key<'static>, String> {
		Key::new("dev.monochange.projects", "client-session-v1").map_err(|e| e.to_string())
	}
	pub async fn restore() -> Result<Option<ClientSession>, String> {
		SecureStore::new()
			.map_err(|e| e.to_string())?
			.get(storage_key()?)
			.await
			.map_err(|e| e.to_string())?
			.map(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
			.transpose()
	}
	pub async fn clear() -> Result<(), String> {
		SecureStore::new()
			.map_err(|e| e.to_string())?
			.delete(storage_key()?)
			.await
			.map_err(|e| e.to_string())
	}
	pub async fn sign_in() -> Result<ClientSession, String> {
		let store = SecureStore::new().map_err(|e| e.to_string())?;
		let key = storage_key()?;
		let crypto = leptoskit_plugin_crypto::guest::Crypto::new().map_err(|e| e.to_string())?;
		let random = crypto.random_bytes(32).await.map_err(|e| e.to_string())?;
		let (verifier, challenge) = monochange_shared::pkce_pair(
			random
				.as_ref()
				.try_into()
				.map_err(|_| "Invalid random bytes")?,
		);
		let state = crypto
			.uuid_v4()
			.await
			.map_err(|e| e.to_string())?
			.to_string();
		let web = WebAuth::new().map_err(|e| e.to_string())?;
		let flow = web.begin().await.map_err(|e| e.to_string())?;
		let api = client(None);
		let url = api.connect_url(
			monochange_shared::ClientKind::Native,
			&flow.return_to,
			&challenge,
			&state,
		);
		let callback = web
			.authenticate(&flow, &url)
			.await
			.map_err(|e| e.to_string())?;
		let request = monochange_shared::client_auth::ClientHandoff {
			client: monochange_shared::ClientKind::Native,
			redirect_uri: flow.return_to,
			state,
			verifier,
		}
		.consume(&callback)
		.map_err(str::to_owned)?;
		let session = api.exchange(request).await.map_err(|e| e.to_string())?;
		let bytes = serde_json::to_vec(&session).map_err(|e| e.to_string())?;
		if let Err(error) = store.set(key, &bytes).await {
			let _ = api.revoke(&session.access_token).await;
			return Err(format!("Your session couldn't be stored securely: {error}"));
		}
		Ok(session)
	}
	pub async fn post(
		url: &str,
		bearer: Option<&str>,
		content_type: &str,
		body: &str,
	) -> Result<HttpResponse, ApiError> {
		use leptoskit_plugin_http::Header;
		use leptoskit_plugin_http::Method;
		use leptoskit_plugin_http::Request;
		use leptoskit_plugin_http::guest::HttpClient;
		let mut headers = vec![Header {
			name: "Content-Type".into(),
			value: content_type.as_bytes().to_vec(),
		}];
		if let Some(token) = bearer {
			headers.push(Header {
				name: "Authorization".into(),
				value: format!("Bearer {token}").into_bytes(),
			});
		}
		let response = HttpClient::connect()
			.map_err(|e| ApiError::Transport(e.to_string()))?
			.execute(Request {
				method: Method::Post,
				url: url.into(),
				headers,
				body: body.as_bytes().to_vec(),
			})
			.await
			.map_err(|e| ApiError::Transport(e.to_string()))?;
		Ok(HttpResponse {
			status: response.status,
			body: String::from_utf8(response.body).map_err(|e| ApiError::Decode(e.to_string()))?,
		})
	}
}
#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
pub use native::clear;
#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
pub use native::post;
#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
pub use native::restore;
#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
pub use native::sign_in;

#[cfg(not(all(target_arch = "wasm32", feature = "native", not(feature = "web"))))]
pub async fn restore() -> Result<Option<ClientSession>, String> {
	Err("Secure device storage is available in the native app".into())
}
#[cfg(not(all(target_arch = "wasm32", feature = "native", not(feature = "web"))))]
pub async fn clear() -> Result<(), String> {
	Err("Secure device storage is unavailable".into())
}
#[cfg(not(all(target_arch = "wasm32", feature = "native", not(feature = "web"))))]
pub async fn sign_in() -> Result<ClientSession, String> {
	Err("Open the installed monochange projects app to sign in".into())
}
#[cfg(not(all(target_arch = "wasm32", feature = "native", not(feature = "web"))))]
async fn post(_: &str, _: Option<&str>, _: &str, _: &str) -> Result<HttpResponse, ApiError> {
	Err(ApiError::Transport("Native HTTP is unavailable".into()))
}

/// Open browser management, which uses the website's cookie and permissions.
#[cfg(all(target_arch = "wasm32", feature = "native", not(feature = "web")))]
pub async fn open_projects() -> Result<(), String> {
	let link = leptoskit_plugin_linking::Link::parse(&format!(
		"{}/projects",
		crate::SERVER.trim_end_matches('/')
	))
	.map_err(|e| e.to_string())?;
	leptoskit_plugin_linking::guest::Linking::new()
		.map_err(|e| e.to_string())?
		.open(&link)
		.await
		.map_err(|e| e.to_string())
}
#[cfg(not(all(target_arch = "wasm32", feature = "native", not(feature = "web"))))]
pub async fn open_projects() -> Result<(), String> {
	Err("Open the monochange projects website in your browser".into())
}
