//! Error types and the shared error page.
//!
//! `ErrorTemplate` lives in the projects entry so both browser surfaces
//! render one implementation; this module re-exports it alongside the
//! website's error enum.

pub use monochange_projects::error::ErrorTemplate;
use thiserror::Error;

/// Application-level error type.
#[derive(Debug, Error)]
#[must_use]
pub enum AppError {
	#[error("Not found: {0}")]
	NotFound(String),

	#[error("Authentication required")]
	Unauthorized,

	#[error("Internal server error: {0}")]
	Internal(String),

	#[error("GitHub API error: {0}")]
	GitHub(String),

	#[error("Database error: {0}")]
	Database(String),
}

impl From<AppError> for u16 {
	fn from(err: AppError) -> Self {
		match err {
			AppError::NotFound(_) => 404,
			AppError::Unauthorized => 401,
			AppError::Internal(_) | AppError::Database(_) => 500,
			AppError::GitHub(_) => 502,
		}
	}
}
