// The tokio and rstest macros construct their test runtimes with `expect`.
#![allow(clippy::disallowed_methods)]

use std::net::IpAddr;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use rstest::rstest;
use server_fn::error::FromServerFnError;
use tower::ServiceExt;

use super::*;

const SECRET: &str = "rate-limit-test-secret";
const SERVER_FN: &str = "/api/get_session123";
const TASKS: &str = "/api/projects/acme/web/tasks";

fn tier(per_minute: u32, burst: u32) -> RateLimitTier {
	RateLimitTier {
		per_minute: NonZeroU32::new(per_minute).unwrap(),
		burst: NonZeroU32::new(burst).unwrap(),
	}
}

fn config(anonymous: RateLimitTier, authenticated: RateLimitTier) -> RateLimitConfig {
	RateLimitConfig {
		anonymous,
		authenticated,
	}
}

fn router(rate_limit: RateLimit) -> Router {
	Router::new()
		.route(SERVER_FN, post(|| async { "session" }))
		.route(TASKS, get(|| async { "tasks" }))
		.route(WEBHOOK_PATH, post(|| async { "delivered" }))
		.route("/dashboard", get(|| async { "page" }))
		.layer(axum::middleware::from_fn_with_state(rate_limit, limit))
}

fn request(method: &str, path: &str, peer: Option<&str>, session: Option<&str>) -> Request<Body> {
	let mut request = Request::builder().method(method).uri(path);
	if let Some(token) = session {
		request = request.header(
			header::COOKIE,
			format!(
				"theme=dark; {}={token}",
				monochange_app_api::oauth::SESSION_COOKIE_NAME
			),
		);
	}
	let mut request = request.body(Body::empty()).unwrap();
	if let Some(peer) = peer {
		request
			.extensions_mut()
			.insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
	}
	request
}

async fn send(router: &Router, request: Request<Body>) -> Response {
	router.clone().oneshot(request).await.unwrap()
}

fn header_value<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
	response
		.headers()
		.get(name)
		.map(|value| value.to_str().unwrap())
}

async fn body(response: Response) -> String {
	let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	String::from_utf8(bytes.to_vec()).unwrap()
}

fn token(user: i32) -> String {
	monochange_app_api::create_token(SECRET, user, i64::from(user) * 10, "octocat").unwrap()
}

#[test]
fn defaults_give_signed_in_users_the_larger_bucket() {
	let config = RateLimitConfig::default();

	assert_eq!(config.anonymous, tier(60, 30));
	assert_eq!(config.authenticated, tier(300, 100));
	assert_eq!(config.anonymous.window_seconds(), 30);
	assert_eq!(config.authenticated.window_seconds(), 20);
	assert_eq!(tier(7, 1).window_seconds(), 9);
}

#[test]
fn unset_or_empty_variables_keep_the_defaults() {
	assert_eq!(
		RateLimitConfig::from_env_reader(|_| None),
		Ok(RateLimitConfig::default())
	);
	assert_eq!(
		RateLimitConfig::from_env_reader(|_| Some("  ".to_owned())),
		Ok(RateLimitConfig::default())
	);
}

#[test]
fn variables_override_each_limit() {
	let config = RateLimitConfig::from_env_reader(|key| {
		match key {
			ANONYMOUS_PER_MINUTE_ENV => Some(" 120 ".to_owned()),
			ANONYMOUS_BURST_ENV => Some("10".to_owned()),
			AUTHENTICATED_PER_MINUTE_ENV => Some("900".to_owned()),
			AUTHENTICATED_BURST_ENV => Some("200".to_owned()),
			_ => None,
		}
	});

	assert_eq!(config, Ok(self::config(tier(120, 10), tier(900, 200))));
}

#[rstest]
#[case("0")]
#[case("-5")]
#[case("lots")]
fn invalid_variables_name_the_variable(#[case] value: &str) {
	let error = RateLimitConfig::from_env_reader(|key| {
		(key == AUTHENTICATED_BURST_ENV).then(|| value.to_owned())
	})
	.unwrap_err();

	assert_eq!(error.key, AUTHENTICATED_BURST_ENV);
	assert_eq!(
		error.to_string(),
		format!(
			"MONOCHANGE_RATE_LIMIT_AUTHENTICATED_BURST must be a whole number greater than zero, got \
			 `{value}`"
		)
	);
}

#[test]
fn the_app_registers_server_functions_under_api() {
	assert!(
		server_fn::axum::server_fn_paths().any(|(path, _)| path.starts_with("/api/get_session")),
		"server function paths moved; the rate limit relies on them"
	);
}

#[tokio::test]
async fn anonymous_clients_spend_a_burst_then_get_429_with_retry_after() {
	let router = router(RateLimit::new(
		config(tier(1, 2), tier(600, 100)),
		SECRET,
		[SERVER_FN],
	));
	let peer = Some("203.0.113.7:4000");

	let first = send(&router, request("GET", TASKS, peer, None)).await;
	assert_eq!(first.status(), StatusCode::OK);
	assert_eq!(
		header_value(&first, "ratelimit-policy"),
		Some("\"anonymous\";q=2;w=120")
	);
	assert_eq!(header_value(&first, "ratelimit"), Some("\"anonymous\";r=1"));
	assert_eq!(header_value(&first, "retry-after"), None);

	let second = send(&router, request("GET", TASKS, peer, None)).await;
	assert_eq!(second.status(), StatusCode::OK);
	assert_eq!(
		header_value(&second, "ratelimit"),
		Some("\"anonymous\";r=0")
	);

	let rejected = send(&router, request("GET", TASKS, peer, None)).await;
	assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
	assert_eq!(header_value(&rejected, "retry-after"), Some("60"));
	assert_eq!(
		header_value(&rejected, "ratelimit"),
		Some("\"anonymous\";r=0;t=60")
	);
	assert_eq!(
		header_value(&rejected, "ratelimit-policy"),
		Some("\"anonymous\";q=2;w=120")
	);
	assert_eq!(
		header_value(&rejected, "content-type"),
		Some("application/json")
	);
	assert_eq!(
		body(rejected).await,
		r#"{"message":"Too many requests. Try again in 60 seconds."}"#
	);

	let elsewhere = send(
		&router,
		request("GET", TASKS, Some("198.51.100.1:4000"), None),
	)
	.await;
	assert_eq!(elsewhere.status(), StatusCode::OK);
}

#[tokio::test]
async fn server_functions_reject_with_an_error_the_leptos_client_decodes() {
	let router = router(RateLimit::new(
		config(tier(60, 1), tier(600, 100)),
		SECRET,
		[SERVER_FN],
	));
	let peer = Some("203.0.113.7:4000");

	assert_eq!(
		send(&router, request("POST", SERVER_FN, peer, None))
			.await
			.status(),
		StatusCode::OK
	);
	let rejected = send(&router, request("POST", SERVER_FN, peer, None)).await;

	assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
	assert_eq!(header_value(&rejected, "retry-after"), Some("1"));
	assert_eq!(
		header_value(&rejected, "content-type"),
		Some("text/plain; charset=utf-8")
	);
	let body = body(rejected).await;
	assert_eq!(
		body,
		"ServerError|Too many requests. Try again in 1 second."
	);
	let decoded: server_fn::ServerFnError = server_fn::ServerFnError::de(body.into());
	assert_eq!(
		decoded,
		server_fn::ServerFnError::ServerError(
			"Too many requests. Try again in 1 second.".to_owned()
		)
	);
}

#[tokio::test]
async fn signed_in_users_get_their_own_larger_bucket_on_any_address() {
	let router = router(RateLimit::new(
		config(tier(1, 1), tier(1, 2)),
		SECRET,
		[SERVER_FN],
	));
	let session = token(7);

	let first = send(
		&router,
		request("POST", SERVER_FN, Some("203.0.113.7:4000"), Some(&session)),
	)
	.await;
	assert_eq!(first.status(), StatusCode::OK);
	assert_eq!(
		header_value(&first, "ratelimit-policy"),
		Some("\"authenticated\";q=2;w=120")
	);
	assert_eq!(
		header_value(&first, "ratelimit"),
		Some("\"authenticated\";r=1")
	);

	// The anonymous bucket of the same address is untouched.
	let anonymous = send(
		&router,
		request("GET", TASKS, Some("203.0.113.7:4000"), None),
	)
	.await;
	assert_eq!(anonymous.status(), StatusCode::OK);

	// The user's bucket follows them to another network.
	let second = send(
		&router,
		request("GET", TASKS, Some("198.51.100.1:4000"), Some(&session)),
	)
	.await;
	assert_eq!(second.status(), StatusCode::OK);
	let third = send(
		&router,
		request("GET", TASKS, Some("198.51.100.1:4000"), Some(&session)),
	)
	.await;
	assert_eq!(third.status(), StatusCode::TOO_MANY_REQUESTS);
	assert_eq!(
		header_value(&third, "ratelimit"),
		Some("\"authenticated\";r=0;t=60")
	);

	// Another user is unaffected.
	let other = send(
		&router,
		request("GET", TASKS, Some("198.51.100.1:4000"), Some(&token(8))),
	)
	.await;
	assert_eq!(other.status(), StatusCode::OK);
}

#[rstest]
#[case::forged("not-a-token".to_owned())]
#[case::wrong_secret(monochange_app_api::create_token("other-secret", 7, 70, "octocat").unwrap())]
#[tokio::test]
async fn invalid_sessions_count_against_the_address(#[case] session: String) {
	let router = router(RateLimit::new(
		config(tier(1, 1), tier(600, 100)),
		SECRET,
		[SERVER_FN],
	));
	let peer = Some("203.0.113.7:4000");

	let first = send(&router, request("GET", TASKS, peer, Some(&session))).await;
	assert_eq!(header_value(&first, "ratelimit"), Some("\"anonymous\";r=0"));
	let second = send(&router, request("GET", TASKS, peer, None)).await;
	assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[rstest]
#[case::webhooks("POST", WEBHOOK_PATH)]
#[case::pages("GET", "/dashboard")]
#[tokio::test]
async fn webhooks_and_pages_are_never_limited(#[case] method: &str, #[case] path: &str) {
	let router = router(RateLimit::new(
		config(tier(1, 1), tier(1, 1)),
		SECRET,
		[SERVER_FN],
	));

	for _ in 0..3 {
		let response = send(
			&router,
			request(method, path, Some("203.0.113.7:4000"), None),
		)
		.await;
		assert_eq!(response.status(), StatusCode::OK);
		assert_eq!(header_value(&response, "ratelimit"), None);
	}
}

#[tokio::test]
async fn requests_without_a_peer_address_share_one_bucket() {
	let router = router(RateLimit::new(
		config(tier(1, 1), tier(600, 100)),
		SECRET,
		[SERVER_FN],
	));

	assert_eq!(
		send(&router, request("GET", TASKS, None, None))
			.await
			.status(),
		StatusCode::OK
	);
	assert_eq!(
		send(&router, request("GET", TASKS, None, None))
			.await
			.status(),
		StatusCode::TOO_MANY_REQUESTS
	);
}

fn address_of(peer: &str, forwarded: &[&str]) -> IpAddr {
	let mut request = request("GET", TASKS, Some(peer), None);
	for value in forwarded {
		request
			.headers_mut()
			.append("x-forwarded-for", value.parse().unwrap());
	}
	client_address(&request)
}

#[rstest]
#[case::public_peer_ignores_the_header("203.0.113.7:4000", &["198.51.100.1"], "203.0.113.7")]
#[case::proxy_peer_uses_the_header("172.18.0.3:4000", &["198.51.100.1"], "198.51.100.1")]
#[case::loopback_peer_uses_the_header("127.0.0.1:4000", &["198.51.100.1"], "198.51.100.1")]
#[case::spoofed_entries_left_of_the_client(
	"172.18.0.3:4000",
	&["1.2.3.4, 198.51.100.1, 10.0.0.2"],
	"198.51.100.1"
)]
#[case::entries_across_header_lines(
	"172.18.0.3:4000",
	&["1.2.3.4", "198.51.100.1"],
	"198.51.100.1"
)]
#[case::private_chain_uses_the_farthest_hop("172.18.0.3:4000", &["192.168.1.5, 10.0.0.2"], "192.168.1.5")]
#[case::garbage_stops_at_the_last_good_hop("172.18.0.3:4000", &["junk, 10.0.0.2"], "10.0.0.2")]
#[case::garbage_alone_falls_back_to_the_peer("172.18.0.3:4000", &["junk"], "172.18.0.3")]
#[case::missing_header_falls_back_to_the_peer("172.18.0.3:4000", &[], "172.18.0.3")]
#[case::mapped_ipv4_peer("[::ffff:203.0.113.7]:4000", &[], "203.0.113.7")]
#[case::ipv6_clients_group_by_slash_64(
	"[2001:db8:1:2:aaaa:bbbb:cccc:dddd]:4000",
	&[],
	"2001:db8:1:2::"
)]
#[case::ipv6_loopback_peer_uses_the_header("[::1]:4000", &["2001:db8::1"], "2001:db8::")]
#[case::ipv6_unique_local_peer_uses_the_header("[fd00::3]:4000", &["198.51.100.1"], "198.51.100.1")]
fn client_addresses_trust_only_proxy_peers(
	#[case] peer: &str,
	#[case] forwarded: &[&str],
	#[case] expected: &str,
) {
	assert_eq!(
		address_of(peer, forwarded),
		expected.parse::<IpAddr>().unwrap()
	);
}

#[rstest]
#[case(Duration::ZERO, 1)]
#[case(Duration::from_millis(1), 1)]
#[case(Duration::from_millis(1500), 2)]
#[case(Duration::from_secs(2), 2)]
fn waits_round_up_to_whole_seconds(#[case] wait: Duration, #[case] expected: u64) {
	assert_eq!(whole_seconds(wait), expected);
}

#[tokio::test]
async fn cleanup_drops_idle_buckets() {
	let rate_limit = RateLimit::new(
		config(tier(60_000, 1), tier(60_000, 1)),
		SECRET,
		[SERVER_FN],
	);
	let router = router(rate_limit.clone());
	send(
		&router,
		request("GET", TASKS, Some("203.0.113.7:4000"), None),
	)
	.await;
	send(&router, request("GET", TASKS, None, Some(&token(7)))).await;
	assert_eq!(rate_limit.limits.anonymous.len(), 1);
	assert_eq!(rate_limit.limits.authenticated.len(), 1);

	let cleanup = rate_limit.spawn_cleanup(Duration::from_millis(5));
	tokio::time::sleep(Duration::from_millis(50)).await;
	cleanup.abort();

	assert!(rate_limit.limits.anonymous.is_empty());
	assert!(rate_limit.limits.authenticated.is_empty());
}
