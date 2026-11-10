//! Client handoff wire contract tests.

#![allow(clippy::disallowed_methods)]

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::Digest;

use crate::ClientKind;
use crate::ClientSession;
use crate::ClientSessionInfo;
use crate::ClientTokenRequest;
use crate::SessionUser;
use crate::code_challenge;
use crate::is_valid_code_challenge;
use crate::is_valid_code_verifier;
use crate::is_valid_state;
use crate::pkce_pair;

fn user() -> SessionUser {
	SessionUser {
		github_id: 42,
		github_login: "monochange".into(),
		github_avatar_url: None,
		plan_tier: "free".into(),
	}
}

#[test]
fn pkce_pair_is_rfc7636_s256() {
	let random = [7_u8; 32];
	let (verifier, challenge) = pkce_pair(random);
	assert_eq!(verifier.len(), 43);
	assert!(
		verifier
			.bytes()
			.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
	);
	assert_eq!(challenge.len(), 43);
	assert_eq!(challenge, code_challenge(&verifier));
	let expected = URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()));
	assert_eq!(challenge, expected);
	assert_ne!(pkce_pair([1_u8; 32]).1, pkce_pair([2_u8; 32]).1);
}

#[test]
fn verifier_and_challenge_syntax_is_bounded() {
	assert!(is_valid_code_verifier(&"a".repeat(43)));
	assert!(is_valid_code_verifier(&"a".repeat(128)));
	assert!(!is_valid_code_verifier(&"a".repeat(42)));
	assert!(!is_valid_code_verifier(&"a".repeat(129)));
	assert!(!is_valid_code_verifier(&("a".repeat(43) + "!/")));

	assert!(is_valid_code_challenge(&"a".repeat(43)));
	assert!(!is_valid_code_challenge(&"a".repeat(42)));
	assert!(!is_valid_code_challenge(&"a".repeat(44)));
	assert!(!is_valid_code_challenge(&("a".repeat(43) + "!")));
}

#[test]
fn state_is_bounded_and_url_safe() {
	assert!(is_valid_state(&"a".repeat(16)));
	assert!(is_valid_state(&"a".repeat(128)));
	assert!(is_valid_state("state-123_abc.~XYZ"));
	assert!(!is_valid_state(&"a".repeat(15)));
	assert!(!is_valid_state(&"a".repeat(129)));
	assert!(!is_valid_state("state with spaces"));
	assert!(!is_valid_state("state?query=1"));
	assert!(!is_valid_state("state#fragment"));
	assert!(!is_valid_state("state&other=1"));
}

#[test]
fn client_kind_round_trips_and_rejects_unknown_names() {
	for (kind, name) in [
		(ClientKind::Native, "native"),
		(ClientKind::Extension, "extension"),
	] {
		assert_eq!(kind.as_str(), name);
		assert_eq!(ClientKind::parse(name), Some(kind));
		let encoded = serde_json::to_string(&kind).unwrap();
		let decoded: ClientKind = serde_json::from_str(&encoded).unwrap();
		assert_eq!(decoded, kind);
		assert_eq!(kind.to_string(), name);
	}
	assert_eq!(ClientKind::parse("web"), None);
	assert_eq!(ClientKind::parse(""), None);
}

#[test]
fn token_request_denies_unknown_fields_and_redacts_secrets() {
	let request = ClientTokenRequest {
		client: ClientKind::Native,
		code: "one-time-code".into(),
		code_verifier: "verifier".into(),
		redirect_uri: "dev.monochange.app:/auth/native/return".into(),
	};
	let encoded = serde_json::to_string(&request).unwrap();
	let decoded: ClientTokenRequest = serde_json::from_str(&encoded).unwrap();
	assert_eq!(decoded.code, "one-time-code");
	assert_eq!(
		decoded.redirect_uri,
		"dev.monochange.app:/auth/native/return"
	);

	let debug = format!("{request:?}");
	assert!(!debug.contains("one-time-code"));
	assert!(!debug.contains("\"verifier\""));
	assert!(debug.contains("[redacted]"));
	assert!(debug.contains("dev.monochange.app:/auth/native/return"));

	let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
	value["extra"] = serde_json::json!("unexpected");
	let text = value.to_string();
	assert!(serde_json::from_str::<ClientTokenRequest>(&text).is_err());
}

#[test]
fn client_session_debug_redacts_the_bearer_token() {
	let session = ClientSession {
		id: "mcs_1".into(),
		access_token: "mcd_secret_bearer".into(),
		expires_at: 1_900_000_000,
		user: user(),
	};
	let debug = format!("{session:?}");
	assert!(!debug.contains("mcd_secret_bearer"));
	assert!(debug.contains("[redacted]"));
	assert!(debug.contains("mcs_1"));
}

#[test]
fn client_session_info_round_trips_json() {
	let info = ClientSessionInfo {
		id: "mcs_1".into(),
		client: ClientKind::Extension,
		created_at: 1_700_000_000,
		last_used_at: Some(1_700_000_100),
		expires_at: 1_900_000_000,
	};
	let encoded = serde_json::to_string(&info).unwrap();
	let decoded: ClientSessionInfo = serde_json::from_str(&encoded).unwrap();
	assert_eq!(decoded.id, "mcs_1");
	assert_eq!(decoded.client, ClientKind::Extension);
	assert_eq!(decoded.last_used_at, Some(1_700_000_100));
}

fn handoff() -> super::ClientHandoff {
	super::ClientHandoff {
		client: ClientKind::Native,
		redirect_uri: "dev.monochange.app:/auth/native/return".into(),
		state: "request-state-123456".into(),
		verifier: "verifier".into(),
	}
}
#[test]
fn handoff_validates_callback_state_and_denial_before_exchange() {
	let valid = "dev.monochange.app:/auth/native/return?code=one-time&state=request-state-123456";
	assert_eq!(handoff().consume(valid).unwrap().code, "one-time");
	for callback in [
		"not a url",
		"other.app:/auth/native/return?code=x&state=request-state-123456",
		"dev.monochange.app:/other?code=x&state=request-state-123456",
		"dev.monochange.app:/auth/native/return?code=x&state=request-state-123456#fragment",
		"dev.monochange.app:/auth/native/return?code=x&state=wrong",
		"dev.monochange.app:/auth/native/return?code=x&state=request-state-123456&state=request-state-123456",
		"dev.monochange.app:/auth/native/return?error=access_denied&state=request-state-123456",
		"dev.monochange.app:/auth/native/return?state=request-state-123456",
		"dev.monochange.app:/auth/native/return?code=&state=request-state-123456",
		"dev.monochange.app:/auth/native/return?code=x&code=y&state=request-state-123456",
	] {
		assert!(handoff().consume(callback).is_err(), "{callback}");
	}
}
