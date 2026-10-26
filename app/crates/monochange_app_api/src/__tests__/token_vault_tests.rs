//! Unit tests for the database token vault.

use super::SEALED_PREFIX;
use super::TokenVault;
use super::TokenVaultError;

#[test]
fn rejects_an_empty_key() {
	assert!(matches!(
		TokenVault::new(""),
		Err(TokenVaultError::EmptyKey)
	));
}

#[test]
fn seals_and_opens_a_token() {
	let vault = TokenVault::new("vault-test-key").unwrap();
	let sealed = vault.seal("gho_test-access-token");

	assert!(sealed.starts_with(SEALED_PREFIX));
	assert_ne!(sealed, "gho_test-access-token");
	assert_eq!(vault.open(&sealed).unwrap(), "gho_test-access-token");
}

#[test]
fn sealing_the_same_token_twice_yields_different_values() {
	let vault = TokenVault::new("vault-test-key").unwrap();
	let first = vault.seal("same-token");
	let second = vault.seal("same-token");

	assert_ne!(first, second, "the nonce must be fresh for every seal");
	assert_eq!(vault.open(&first).unwrap(), "same-token");
	assert_eq!(vault.open(&second).unwrap(), "same-token");
}

#[test]
fn passes_plaintext_rows_through_unchanged() {
	let vault = TokenVault::new("vault-test-key").unwrap();

	assert_eq!(
		vault.open("legacy-access-token").unwrap(),
		"legacy-access-token"
	);
	assert!(!TokenVault::is_sealed("legacy-access-token"));
	assert!(TokenVault::is_sealed(&vault.seal("token")));
}

#[test]
fn rejects_a_tampered_ciphertext() {
	let vault = TokenVault::new("vault-test-key").unwrap();
	let sealed = vault.seal("gho_test-access-token");

	// Flip a byte inside the base64 payload; base64's trailing padding bits
	// could absorb a change to the final character, so the middle is safer.
	let mut tampered = sealed.clone();
	let middle = tampered.len() / 2;
	let replacement = if tampered.as_bytes()[middle] == b'A' {
		'B'
	} else {
		'A'
	};
	tampered.replace_range(middle..=middle, &replacement.to_string());

	assert_eq!(vault.open(&tampered), Err(TokenVaultError::Unseal));
}

#[test]
fn rejects_a_ciphertext_sealed_under_a_different_key() {
	let sealing = TokenVault::new("first-key").unwrap();
	let opening = TokenVault::new("second-key").unwrap();

	let sealed = sealing.seal("gho_test-access-token");
	assert_eq!(opening.open(&sealed), Err(TokenVaultError::Unseal));
}

#[test]
fn rejects_malformed_prefixed_values() {
	let vault = TokenVault::new("vault-test-key").unwrap();

	assert_eq!(
		vault.open("v1:not-base64!"),
		Err(TokenVaultError::Malformed)
	);
	assert_eq!(
		vault.open("v1:only-one-part"),
		Err(TokenVaultError::Malformed)
	);
}
