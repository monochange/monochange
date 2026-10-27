//! Per-client rate limits for Leptos server functions, REST endpoints under
//! `/api`, and the OAuth pages that initiate provider work during SSR.
//!
//! Every limit is a [`governor`] GCRA bucket: a client may spend a burst of
//! requests at once, and the bucket refills one request at a time at the
//! sustained rate. Signed-in users get a larger bucket keyed by their user id,
//! so a busy dashboard never competes with other people behind the same
//! network address. Everyone else shares a smaller bucket per client address.
//!
//! `/login` and `/auth/callback` always spend the address bucket, including
//! when the browser carries a session. Other pages, hydration assets,
//! `/health`, and GitHub webhooks are not limited.
//! GitHub never redelivers a dropped webhook on its own and every delivery is
//! already authenticated by its signature, so limiting it would only lose
//! events.
//!
//! Each limited response carries the IETF `RateLimit-Policy` and `RateLimit`
//! fields, and a rejection adds `Retry-After`. Server functions reject with an
//! encoded `ServerFnError`, so the Leptos client shows the message instead of
//! failing to decode the body; REST endpoints reject with the same JSON
//! `{"message": ...}` body as their other errors. OAuth pages return plain text.

use std::collections::HashSet;
use std::hash::Hash;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::extract::Request;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::HeaderName;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::http::header;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use axum_extra::extract::cookie::CookieJar;
use governor::Quota;
use governor::RateLimiter;
use governor::clock::Clock;
use governor::clock::DefaultClock;
use governor::middleware::StateInformationMiddleware;
use governor::state::keyed::DefaultKeyedStateStore;
use monochange_app_api::release_api::ApiError;
use server_fn::error::FromServerFnError;

/// Sustained requests per minute for one anonymous client address.
pub const ANONYMOUS_PER_MINUTE_ENV: &str = "MONOCHANGE_RATE_LIMIT_ANONYMOUS_PER_MINUTE";
/// Requests one anonymous client address may make at once.
pub const ANONYMOUS_BURST_ENV: &str = "MONOCHANGE_RATE_LIMIT_ANONYMOUS_BURST";
/// Sustained requests per minute for one signed-in user.
pub const AUTHENTICATED_PER_MINUTE_ENV: &str = "MONOCHANGE_RATE_LIMIT_AUTHENTICATED_PER_MINUTE";
/// Requests one signed-in user may make at once.
pub const AUTHENTICATED_BURST_ENV: &str = "MONOCHANGE_RATE_LIMIT_AUTHENTICATED_BURST";

/// How often idle buckets are dropped, so the key stores don't grow with
/// every address that ever called the API.
pub const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// GitHub's webhook deliveries, exempt from limits.
const WEBHOOK_PATH: &str = "/api/github/webhooks";

const RATELIMIT_POLICY: HeaderName = HeaderName::from_static("ratelimit-policy");
const RATELIMIT: HeaderName = HeaderName::from_static("ratelimit");

/// One bucket: the sustained rate and the burst a client may spend at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitTier {
	pub per_minute: NonZeroU32,
	pub burst: NonZeroU32,
}

impl RateLimitTier {
	const fn new(per_minute: u32, burst: u32) -> Self {
		Self {
			per_minute: NonZeroU32::new(per_minute).unwrap(),
			burst: NonZeroU32::new(burst).unwrap(),
		}
	}

	fn quota(self) -> Quota {
		Quota::per_minute(self.per_minute).allow_burst(self.burst)
	}

	/// Seconds an empty bucket takes to refill, the `w` of `RateLimit-Policy`.
	fn window_seconds(self) -> u64 {
		(u64::from(self.burst.get()) * 60).div_ceil(u64::from(self.per_minute.get()))
	}
}

/// The limits for anonymous clients and for signed-in users.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitConfig {
	pub anonymous: RateLimitTier,
	pub authenticated: RateLimitTier,
}

impl Default for RateLimitConfig {
	/// One request a second with a burst of 30 for anonymous clients, and five
	/// a second with a burst of 100 for signed-in users. A page load or a
	/// client-side navigation makes a handful of server function calls, so
	/// neither tier is felt by people using the site; a script polling the
	/// public API settles at one request a second.
	fn default() -> Self {
		Self {
			anonymous: RateLimitTier::new(60, 30),
			authenticated: RateLimitTier::new(300, 100),
		}
	}
}

/// A rate limit variable that isn't a whole number above zero.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{key} must be a whole number greater than zero, got `{value}`")]
pub struct RateLimitConfigError {
	pub key: &'static str,
	pub value: String,
}

impl RateLimitConfig {
	/// Read the limits from the process environment.
	pub fn from_env() -> Result<Self, RateLimitConfigError> {
		Self::from_env_reader(|key| std::env::var(key).ok())
	}

	/// Read the limits from an injected environment reader. Unset or empty
	/// variables keep their defaults.
	pub fn from_env_reader(
		get: impl Fn(&str) -> Option<String>,
	) -> Result<Self, RateLimitConfigError> {
		let defaults = Self::default();
		let read = |key: &'static str, default: NonZeroU32| {
			match get(key).filter(|value| !value.trim().is_empty()) {
				None => Ok(default),
				Some(value) => {
					value
						.trim()
						.parse()
						.map_err(|_| RateLimitConfigError { key, value })
				}
			}
		};

		Ok(Self {
			anonymous: RateLimitTier {
				per_minute: read(ANONYMOUS_PER_MINUTE_ENV, defaults.anonymous.per_minute)?,
				burst: read(ANONYMOUS_BURST_ENV, defaults.anonymous.burst)?,
			},
			authenticated: RateLimitTier {
				per_minute: read(
					AUTHENTICATED_PER_MINUTE_ENV,
					defaults.authenticated.per_minute,
				)?,
				burst: read(AUTHENTICATED_BURST_ENV, defaults.authenticated.burst)?,
			},
		})
	}
}

type Limiter<K> =
	RateLimiter<K, DefaultKeyedStateStore<K>, DefaultClock, StateInformationMiddleware>;

/// Who a request is counted against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Client {
	/// A signed-in user, by user id.
	User(i32),
	/// An anonymous client, by network address.
	Address(IpAddr),
}

/// How a limited route reports a rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Surface {
	ServerFunction,
	Rest,
	OAuth,
}

/// The outcome of counting one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Decision {
	policy: &'static str,
	tier: RateLimitTier,
	remaining: u32,
	/// Whole seconds until the client may retry, when the request is rejected.
	retry_after: Option<u64>,
}

impl Decision {
	fn headers(&self) -> HeaderMap {
		let mut headers = HeaderMap::new();
		let policy = format!(
			"\"{}\";q={};w={}",
			self.policy,
			self.tier.burst,
			self.tier.window_seconds()
		);
		let limit = match self.retry_after {
			Some(seconds) => {
				headers.insert(header::RETRY_AFTER, HeaderValue::from(seconds));
				format!("\"{}\";r={};t={seconds}", self.policy, self.remaining)
			}
			None => format!("\"{}\";r={}", self.policy, self.remaining),
		};
		for (name, value) in [(RATELIMIT_POLICY, policy), (RATELIMIT, limit)] {
			if let Ok(value) = HeaderValue::from_str(&value) {
				headers.insert(name, value);
			}
		}
		headers
	}

	fn reject(&self, surface: Surface, seconds: u64) -> Response {
		let unit = if seconds == 1 { "second" } else { "seconds" };
		let message = format!("Too many requests. Try again in {seconds} {unit}.");
		let mut response = match surface {
			Surface::ServerFunction => {
				let body = server_fn::ServerFnError::new(message).ser();
				(
					[(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
					Body::from(body),
				)
					.into_response()
			}
			Surface::Rest => Json(ApiError { message }).into_response(),
			Surface::OAuth => message.into_response(),
		};
		*response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
		response.headers_mut().extend(self.headers());
		response
	}
}

struct Limits {
	config: RateLimitConfig,
	jwt_secret: String,
	server_fn_paths: HashSet<&'static str>,
	anonymous: Limiter<IpAddr>,
	authenticated: Limiter<i32>,
}

/// Shared rate limit state for [`limit`].
#[derive(Clone)]
pub struct RateLimit {
	limits: Arc<Limits>,
}

impl RateLimit {
	/// Limits signed with the session `jwt_secret`. `server_fn_paths` are the
	/// registered Leptos server function paths, which reject in the encoding
	/// the Leptos client decodes.
	pub fn new(
		config: RateLimitConfig,
		jwt_secret: impl Into<String>,
		server_fn_paths: impl IntoIterator<Item = &'static str>,
	) -> Self {
		Self {
			limits: Arc::new(Limits {
				config,
				jwt_secret: jwt_secret.into(),
				server_fn_paths: server_fn_paths.into_iter().collect(),
				anonymous: RateLimiter::keyed(config.anonymous.quota())
					.with_middleware::<StateInformationMiddleware>(),
				authenticated: RateLimiter::keyed(config.authenticated.quota())
					.with_middleware::<StateInformationMiddleware>(),
			}),
		}
	}

	/// Drop idle buckets every `interval` for as long as the server runs.
	pub fn spawn_cleanup(&self, interval: Duration) -> tokio::task::JoinHandle<()> {
		let limits = self.limits.clone();
		tokio::spawn(async move {
			let mut ticks = tokio::time::interval(interval);
			loop {
				ticks.tick().await;
				limits.retain_recent();
			}
		})
	}

	fn surface(&self, path: &str) -> Option<Surface> {
		// The SSR fallback parses this absolute URL before matching its routes.
		// Use the same normalization so dot-segment aliases cannot skip the bucket.
		let rendered_url = format!("http://leptos.dev{path}");
		let oauth_page = leptos_router::location::RequestUrl::new(&rendered_url)
			.parse()
			.is_ok_and(|url| {
				matches!(
					url.path().trim_end_matches('/'),
					"/login" | "/auth/callback"
				)
			});

		if oauth_page {
			Some(Surface::OAuth)
		} else if self.limits.server_fn_paths.contains(path) {
			Some(Surface::ServerFunction)
		} else if path.starts_with("/api/") && path != WEBHOOK_PATH {
			Some(Surface::Rest)
		} else {
			None
		}
	}

	fn check(&self, request: &Request, surface: Surface) -> Decision {
		let limits = &self.limits;
		let client = if surface == Surface::OAuth {
			Client::Address(client_address(request))
		} else {
			client(request, &limits.jwt_secret)
		};

		match client {
			Client::User(id) => {
				count(
					&limits.authenticated,
					&id,
					"authenticated",
					limits.config.authenticated,
				)
			}
			Client::Address(address) => {
				count(
					&limits.anonymous,
					&address,
					"anonymous",
					limits.config.anonymous,
				)
			}
		}
	}
}

impl Limits {
	fn retain_recent(&self) {
		self.anonymous.retain_recent();
		self.anonymous.shrink_to_fit();
		self.authenticated.retain_recent();
		self.authenticated.shrink_to_fit();
	}
}

fn count<K: Clone + Hash + Eq>(
	limiter: &Limiter<K>,
	key: &K,
	policy: &'static str,
	tier: RateLimitTier,
) -> Decision {
	match limiter.check_key(key) {
		Ok(snapshot) => {
			Decision {
				policy,
				tier,
				remaining: snapshot.remaining_burst_capacity(),
				retry_after: None,
			}
		}
		Err(not_until) => {
			let wait = not_until.wait_time_from(limiter.clock().now());
			Decision {
				policy,
				tier,
				remaining: 0,
				retry_after: Some(whole_seconds(wait)),
			}
		}
	}
}

/// A wait rounded up to whole seconds, never zero: `Retry-After` has
/// one-second precision and a client told `0` would retry straight away.
fn whole_seconds(wait: Duration) -> u64 {
	(wait.as_millis().div_ceil(1000) as u64).max(1)
}

/// Count API and OAuth page requests before server functions or SSR run,
/// answering `429 Too Many Requests` once the caller's bucket is empty.
pub async fn limit(State(rate_limit): State<RateLimit>, request: Request, next: Next) -> Response {
	let Some(surface) = rate_limit.surface(request.uri().path()) else {
		return next.run(request).await;
	};
	let decision = rate_limit.check(&request, surface);
	if let Some(seconds) = decision.retry_after {
		tracing::debug!(
			policy = decision.policy,
			path = request.uri().path(),
			"rate limited"
		);
		return decision.reject(surface, seconds);
	}
	let mut response = next.run(request).await;
	response.headers_mut().extend(decision.headers());
	response
}

/// The signed-in user when the request carries a valid session token, and the
/// client address otherwise. Only the token's signature and expiry are
/// checked: the server alone can sign one, so a forged or expired cookie
/// falls back to the address and can't mint fresh buckets.
fn client(request: &Request, jwt_secret: &str) -> Client {
	CookieJar::from_headers(request.headers())
		.get(monochange_app_api::oauth::SESSION_COOKIE_NAME)
		.and_then(|cookie| monochange_app_api::verify_token(jwt_secret, cookie.value()).ok())
		.map_or_else(
			|| Client::Address(client_address(request)),
			|claims| Client::User(claims.sub),
		)
}

/// The address a request came from, grouped so one client gets one bucket.
///
/// Production runs behind Caddy, which replaces any `X-Forwarded-For` it
/// receives from the internet with the connecting address. The header is only
/// believed when the peer is a loopback or private address, which only a
/// proxy on the host or the Docker network can be, and it is read from the
/// right so entries a client wrote itself are never reached. IPv6 clients are
/// grouped by their /64, the smallest block a host is usually given, so one
/// machine can't escape its limit by rotating addresses.
fn client_address(request: &Request) -> IpAddr {
	let Some(ConnectInfo(peer)) = request.extensions().get::<ConnectInfo<SocketAddr>>() else {
		return IpAddr::V4(Ipv4Addr::UNSPECIFIED);
	};
	let peer = peer.ip().to_canonical();
	let address = if is_trusted_proxy(peer) {
		forwarded_client(request.headers()).unwrap_or(peer)
	} else {
		peer
	};
	match address {
		IpAddr::V6(address) => IpAddr::V6(Ipv6Addr::from(u128::from(address) & (u128::MAX << 64))),
		IpAddr::V4(_) => address,
	}
}

/// The nearest `X-Forwarded-For` entry that isn't a trusted proxy.
fn forwarded_client(headers: &HeaderMap) -> Option<IpAddr> {
	let entries = headers
		.get_all("x-forwarded-for")
		.iter()
		.filter_map(|value| value.to_str().ok())
		.flat_map(|value| value.split(','))
		.collect::<Vec<_>>();
	let mut nearest = None;
	for entry in entries.into_iter().rev() {
		let Ok(address) = entry.trim().parse::<IpAddr>() else {
			break;
		};
		let address = address.to_canonical();
		nearest = Some(address);
		if !is_trusted_proxy(address) {
			break;
		}
	}
	nearest
}

fn is_trusted_proxy(address: IpAddr) -> bool {
	match address {
		IpAddr::V4(address) => address.is_loopback() || address.is_private(),
		IpAddr::V6(address) => address.is_loopback() || address.is_unique_local(),
	}
}

#[cfg(test)]
#[path = "__tests__/rate_limit_tests.rs"]
mod tests;
