//! Encryption at rest for the GitHub OAuth tokens stored in the app database.
//!
//! A database file disclosure must not hand out live GitHub user tokens, so
//! every token written to `users` is sealed with AES-256-GCM under a key the
//! deployment configures as `TOKEN_ENCRYPTION_KEY`. Sealed values carry the
//! `v1:` prefix followed by the base64 nonce and ciphertext, so rows written
//! before the vault existed (plaintext) are still readable and are replaced
//! the next time the token is refreshed.

#[cfg(test)]
#[path = "__tests__/token_vault_tests.rs"]
mod tests;

use aes_gcm::Aes256Gcm;
use aes_gcm::Nonce;
use aes_gcm::aead::Aead;
use aes_gcm::aead::AeadCore;
use aes_gcm::aead::KeyInit;
use aes_gcm::aead::Payload;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::Digest;
use sha2::Sha256;
use thiserror::Error;

/// Prefix on every value sealed by this vault version.
pub const SEALED_PREFIX: &str = "v1:";

/// Length of the AES-GCM nonce in sealed values.
const NONCE_LENGTH: usize = 12;

/// Bind sealed values to this vault so a ciphertext cannot be replayed as a
/// different kind of secret.
const SEALED_AAD: &[u8] = b"monochange_app token vault v1";

/// Failures raised while opening the vault or unsealing a stored value.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TokenVaultError {
	#[error("token encryption key is empty; set TOKEN_ENCRYPTION_KEY before starting the server")]
	EmptyKey,
	#[error("stored token has an unreadable `{SEALED_PREFIX}` value")]
	Malformed,
	#[error(
		"stored token could not be unsealed; it was written under a different TOKEN_ENCRYPTION_KEY"
	)]
	Unseal,
}

/// Seals and opens GitHub user tokens for database storage.
#[derive(Clone)]
pub struct TokenVault {
	cipher: Aes256Gcm,
}

impl std::fmt::Debug for TokenVault {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.debug_struct("TokenVault").finish_non_exhaustive()
	}
}

impl TokenVault {
	/// Derive the vault key from the configured secret.
	///
	/// Any non-empty string is accepted: the SHA-256 digest of the secret is
	/// the AES-256 key, so development defaults and generated values work
	/// without a fixed-length encoding.
	pub fn new(secret: &str) -> Result<Self, TokenVaultError> {
		if secret.is_empty() {
			return Err(TokenVaultError::EmptyKey);
		}

		Ok(Self {
			cipher: Aes256Gcm::new(&Sha256::digest(secret.as_bytes())),
		})
	}

	/// Seal a token for storage. The nonce is fresh for every call, so the
	/// same token seals to a different value each time.
	pub fn seal(&self, token: &str) -> String {
		let nonce = Aes256Gcm::generate_nonce(&mut aes_gcm::aead::OsRng);
		let ciphertext = self
			.cipher
			.encrypt(
				&nonce,
				Payload {
					msg: token.as_bytes(),
					aad: SEALED_AAD,
				},
			)
			.unwrap_or_else(|error| panic!("AES-GCM encryption is infallible: {error}"));
		format!(
			"{SEALED_PREFIX}{}:{}",
			STANDARD.encode(nonce),
			STANDARD.encode(ciphertext)
		)
	}

	/// Open a stored token value.
	///
	/// Values without the [`SEALED_PREFIX`] are rows written before the vault
	/// existed; they are returned unchanged and replaced the next time the
	/// token is refreshed.
	pub fn open(&self, stored: &str) -> Result<String, TokenVaultError> {
		let Some(sealed) = stored.strip_prefix(SEALED_PREFIX) else {
			return Ok(stored.to_string());
		};

		let (nonce, ciphertext) = sealed.split_once(':').ok_or(TokenVaultError::Malformed)?;
		let nonce = STANDARD
			.decode(nonce)
			.map_err(|_| TokenVaultError::Malformed)?;
		let ciphertext = STANDARD
			.decode(ciphertext)
			.map_err(|_| TokenVaultError::Malformed)?;
		let nonce: [u8; NONCE_LENGTH] = nonce.try_into().map_err(|_| TokenVaultError::Malformed)?;
		let plaintext = self
			.cipher
			.decrypt(
				&Nonce::from(nonce),
				Payload {
					msg: &ciphertext,
					aad: SEALED_AAD,
				},
			)
			.map_err(|_| TokenVaultError::Unseal)?;
		String::from_utf8(plaintext).map_err(|_| TokenVaultError::Malformed)
	}

	/// Whether a stored value is sealed by this vault.
	#[must_use]
	pub fn is_sealed(stored: &str) -> bool {
		stored.starts_with(SEALED_PREFIX)
	}
}
