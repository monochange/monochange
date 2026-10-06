//! Error types and error template component.

use leptos::prelude::*;
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

/// Error page component.
#[component]
pub fn ErrorTemplate(
	#[prop(default = 500)] status: u16,
	#[prop(default = "Something went wrong.")] message: &'static str,
) -> impl IntoView {
	let title = move || {
		match status {
			404 => "Page not found",
			401 => "Not authorized",
			_ => "Server error",
		}
	};

	view! {
		<div class="site-width status-page">
			<div class="text-center">
				<img src="/branding/mark.svg" width="64" height="64" alt="" class="mx-auto" />
				<h1>
					{status}
				</h1>
				<h2>
					{title}
				</h2>
				<p>{message}</p>
				<div class="actions">
					<a
						href="/"
						class="button button-brand"
					>
						<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
							<path stroke-linecap="round" stroke-linejoin="round" d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18" />
						</svg>
						Go home
					</a>
				</div>
			</div>
		</div>
	}
}
