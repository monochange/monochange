#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::Mutex;

use super::ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV;
use super::ACTIONS_ID_TOKEN_REQUEST_URL_ENV;
use super::ActionsOidcContext;
use super::PUB_DEV_AUDIENCE;
use super::actions_id_token_request_url;
use super::actions_oidc_context;
use super::mint_actions_id_token;
use crate::registry_client;

fn env_map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
	entries
		.iter()
		.map(|(key, value)| ((*key).to_string(), (*value).to_string()))
		.collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn actions_oidc_context_requires_both_endpoint_variables() {
	assert_eq!(actions_oidc_context(&BTreeMap::new()), None);

	let only_url = env_map(&[(
		ACTIONS_ID_TOKEN_REQUEST_URL_ENV,
		"https://runner.example/tokens",
	)]);
	assert_eq!(actions_oidc_context(&only_url), None);

	let only_token = env_map(&[(ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV, "runner-secret")]);
	assert_eq!(actions_oidc_context(&only_token), None);

	let both = env_map(&[
		(
			ACTIONS_ID_TOKEN_REQUEST_URL_ENV,
			"https://runner.example/tokens",
		),
		(ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV, "runner-secret"),
	]);
	assert_eq!(
		actions_oidc_context(&both),
		Some(ActionsOidcContext {
			request_url: "https://runner.example/tokens".to_string(),
			request_token: "runner-secret".to_string(),
		})
	);
}

#[test]
fn actions_oidc_context_ignores_blank_endpoint_values() {
	let blank_url = env_map(&[
		(ACTIONS_ID_TOKEN_REQUEST_URL_ENV, "   "),
		(ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV, "runner-secret"),
	]);
	assert_eq!(actions_oidc_context(&blank_url), None);

	let blank_token = env_map(&[
		(
			ACTIONS_ID_TOKEN_REQUEST_URL_ENV,
			"https://runner.example/tokens",
		),
		(ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV, ""),
	]);
	assert_eq!(actions_oidc_context(&blank_token), None);
}

#[test]
fn actions_id_token_request_url_encodes_the_pub_dev_audience() {
	assert_eq!(
		actions_id_token_request_url("https://runner.example/tokens?other=1", PUB_DEV_AUDIENCE),
		"https://runner.example/tokens?other=1&audience=https%3A%2F%2Fpub.dev"
	);
}

fn http_ok_response(body: &str) -> String {
	format!(
		"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
		body.len()
	)
}

/// Serve `request_count` HTTP responses and capture the raw requests.
fn spawn_oidc_server(
	request_count: usize,
	response: String,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
	let listener = TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind oidc server: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("oidc server address: {error}"));
	let captured = Arc::new(Mutex::new(Vec::new()));
	let captured_clone = Arc::clone(&captured);
	let handle = std::thread::spawn(move || {
		for _ in 0..request_count {
			let Ok((mut stream, _)) = listener.accept() else {
				break;
			};
			let mut request = Vec::new();
			let mut buffer = [0_u8; 512];
			while let Ok(read) = stream.read(&mut buffer) {
				if read == 0 {
					break;
				}
				request.extend_from_slice(&buffer[..read]);
				if String::from_utf8_lossy(&request).contains("\r\n\r\n") {
					break;
				}
			}
			captured_clone
				.lock()
				.unwrap()
				.push(String::from_utf8_lossy(&request).into_owned());
			let _ = stream.write_all(response.as_bytes());
			let _ = stream.flush();
		}
	});
	(format!("http://{address}"), captured, handle)
}

#[tokio::test(flavor = "multi_thread")]
async fn mint_actions_id_token_posts_pub_dev_audience_request_and_yields_the_jwt() {
	let body = r#"{"count":1,"value":"fresh-actions-jwt"}"#;
	let (base_url, captured, server) = spawn_oidc_server(1, http_ok_response(body));
	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));
	let context = ActionsOidcContext {
		request_url: format!("{base_url}/token"),
		request_token: "runner-secret".to_string(),
	};

	let token = mint_actions_id_token(&client, &context)
		.await
		.unwrap_or_else(|error| panic!("mint token: {error}"));
	assert_eq!(token, "fresh-actions-jwt");

	let requests = captured.lock().unwrap().clone();
	assert_eq!(requests.len(), 1);
	assert!(
		requests[0].contains("GET /token&audience=https%3A%2F%2Fpub.dev HTTP/1.1"),
		"unexpected request: {}",
		requests[0]
	);
	assert!(
		requests[0]
			.to_ascii_lowercase()
			.contains("bearer runner-secret"),
		"expected the runner bearer token in the request: {}",
		requests[0]
	);
	server
		.join()
		.unwrap_or_else(|_| panic!("oidc server thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn mint_actions_id_token_reports_http_failures() {
	let response =
		"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
			.to_string();
	let (base_url, captured, server) = spawn_oidc_server(1, response);
	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));
	let context = ActionsOidcContext {
		request_url: format!("{base_url}/token"),
		request_token: "runner-secret".to_string(),
	};

	let error = mint_actions_id_token(&client, &context)
		.await
		.expect_err("minting should fail on HTTP 500");
	assert!(
		error
			.render()
			.contains("failed with HTTP 500 Internal Server Error"),
		"unexpected error: {error}"
	);
	assert!(error.render().contains("permissions: id-token: write"));
	assert_eq!(captured.lock().unwrap().len(), 1);
	server
		.join()
		.unwrap_or_else(|_| panic!("oidc server thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn mint_actions_id_token_requires_the_value_field() {
	let (base_url, _captured, server) = spawn_oidc_server(1, http_ok_response(r#"{"count":0}"#));
	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));
	let context = ActionsOidcContext {
		request_url: format!("{base_url}/token"),
		request_token: "runner-secret".to_string(),
	};

	let error = mint_actions_id_token(&client, &context)
		.await
		.expect_err("missing value field should fail");
	assert!(
		error
			.render()
			.contains("does not contain a `value` token field"),
		"unexpected error: {error}"
	);
	server
		.join()
		.unwrap_or_else(|_| panic!("oidc server thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn mint_actions_id_token_rejects_malformed_responses() {
	let (base_url, _captured, server) = spawn_oidc_server(1, http_ok_response("not json"));
	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));
	let context = ActionsOidcContext {
		request_url: format!("{base_url}/token"),
		request_token: "runner-secret".to_string(),
	};

	let error = mint_actions_id_token(&client, &context)
		.await
		.expect_err("malformed json should fail");
	assert!(
		error
			.render()
			.contains("failed to decode the GitHub Actions OIDC response"),
		"unexpected error: {error}"
	);
	server
		.join()
		.unwrap_or_else(|_| panic!("oidc server thread"));
}

#[tokio::test(flavor = "multi_thread")]
async fn mint_actions_id_token_reports_transport_failures() {
	let listener = TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind probe listener: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("probe address: {error}"));
	drop(listener);

	let client = registry_client().unwrap_or_else(|error| panic!("registry client: {error}"));
	let context = ActionsOidcContext {
		request_url: format!("http://{address}/token"),
		request_token: "runner-secret".to_string(),
	};

	let error = mint_actions_id_token(&client, &context)
		.await
		.expect_err("unreachable endpoint should fail");
	assert!(
		error
			.render()
			.contains("GitHub Actions OIDC token request for"),
		"unexpected error: {error}"
	);
}
