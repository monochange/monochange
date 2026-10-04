//! GitHub App webhook receiver.
//!
//! The monochange GitHub App points its webhooks at `POST /api/github/webhooks`.
//! Installation events keep the app's repository list in sync: an install adds
//! its repositories, a removal deletes them. Every payload is verified with
//! the app webhook secret before it is trusted.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use serde::Deserialize;
use serde::Serialize;

use crate::AppState;
use crate::github_app::InstallationRepository;

/// Error response body for webhook failures.
#[derive(Debug, Serialize)]
pub struct WebhookError {
	pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
enum WebhookEvent {
	#[serde(rename = "created")]
	Created {
		installation: InstallationPayload,
		#[serde(default)]
		repositories: Vec<WebhookRepository>,
	},
	#[serde(rename = "added")]
	Added {
		installation: InstallationPayload,
		#[serde(default, rename = "repositories_added")]
		repositories_added: Vec<WebhookRepository>,
	},
	#[serde(rename = "removed")]
	Removed {
		installation: InstallationPayload,
		#[serde(default, rename = "repositories_removed")]
		repositories_removed: Vec<WebhookRepository>,
	},
	#[serde(rename = "deleted")]
	Deleted { installation: InstallationPayload },
	#[serde(rename = "suspend")]
	Suspend { installation: InstallationPayload },
	#[serde(rename = "unsuspend")]
	Unsuspend { installation: InstallationPayload },
	#[serde(other)]
	Ignored,
}

#[derive(Debug, Deserialize)]
struct InstallationPayload {
	id: i64,
	#[serde(default)]
	account: Option<InstallationAccount>,
}

#[derive(Debug, Deserialize)]
struct InstallationAccount {
	login: Option<String>,
	#[serde(default)]
	r#type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WebhookRepository {
	id: i64,
	#[serde(default)]
	full_name: Option<String>,
	#[serde(default)]
	private: bool,
}

/// `POST /api/github/webhooks` — installation lifecycle events.
pub async fn github_webhook(
	State(state): State<AppState>,
	headers: HeaderMap,
	body: axum::body::Bytes,
) -> Result<StatusCode, (StatusCode, Json<WebhookError>)> {
	let Some(app) = state.github_app.as_ref() else {
		return Ok(StatusCode::OK);
	};
	if !app.webhook_configured() {
		return Err((
			StatusCode::SERVICE_UNAVAILABLE,
			Json(WebhookError {
				message: "webhook secret is not configured".to_string(),
			}),
		));
	}
	let signature = headers
		.get("x-hub-signature-256")
		.and_then(|value| value.to_str().ok())
		.unwrap_or_default();
	app.verify_webhook_signature(&body, signature)
		.map_err(|_| {
			(
				StatusCode::UNAUTHORIZED,
				Json(WebhookError {
					message: "invalid webhook signature".to_string(),
				}),
			)
		})?;

	let event: WebhookEvent = serde_json::from_slice(&body).map_err(|error| {
		(
			StatusCode::BAD_REQUEST,
			Json(WebhookError {
				message: format!("invalid webhook payload: {error}"),
			}),
		)
	})?;

	match event {
		WebhookEvent::Created {
			installation,
			repositories,
		} => {
			sync_installation(
				&state,
				&installation,
				repositories
					.into_iter()
					.map(WebhookRepository::into_repo)
					.collect(),
				true,
			)
			.await
		}
		WebhookEvent::Added {
			installation,
			repositories_added,
		} => {
			sync_installation(
				&state,
				&installation,
				repositories_added
					.into_iter()
					.map(WebhookRepository::into_repo)
					.collect(),
				true,
			)
			.await
		}
		WebhookEvent::Removed {
			installation,
			repositories_removed,
		} => {
			sync_installation(
				&state,
				&installation,
				repositories_removed
					.into_iter()
					.map(WebhookRepository::into_repo)
					.collect(),
				false,
			)
			.await
		}
		WebhookEvent::Deleted { installation } => {
			delete_installation_repositories(&state, installation.id).await
		}
		WebhookEvent::Suspend { installation } => {
			set_installation_suspended(&state, installation.id, true).await
		}
		WebhookEvent::Unsuspend { installation } => {
			set_installation_suspended(&state, installation.id, false).await
		}
		WebhookEvent::Ignored => Ok(StatusCode::OK),
	}
}

impl WebhookRepository {
	fn into_repo(self) -> InstallationRepository {
		InstallationRepository {
			id: self.id,
			name: self
				.full_name
				.as_deref()
				.and_then(|full| full.rsplit('/').next())
				.unwrap_or("unknown")
				.to_string(),
			full_name: self
				.full_name
				.unwrap_or_else(|| "unknown/unknown".to_string()),
			private: self.private,
		}
	}
}

async fn sync_installation(
	state: &AppState,
	installation: &InstallationPayload,
	repositories: Vec<InstallationRepository>,
	add: bool,
) -> Result<StatusCode, (StatusCode, Json<WebhookError>)> {
	let account = installation.account.as_ref();
	let login = account
		.and_then(|account| account.login.clone())
		.unwrap_or_else(|| "unknown".to_string());
	let account_type = account
		.and_then(|account| account.r#type.clone())
		.unwrap_or_else(|| "User".to_string());

	let installation_id: i32 =
		sqlx::query_scalar("SELECT id FROM installations WHERE github_installation_id = $1")
			.bind(installation.id)
			.fetch_optional(&state.db)
			.await
			.map_err(|error| db_error(&error))?
			.unwrap_or(0);

	// The app installs before any user session exists; user_id 0 is the
	// unclaimed placeholder an installer claims after signing in.
	let installation_id = if installation_id == 0 {
		let id: i32 = sqlx::query_scalar(
			"INSERT INTO installations (user_id, github_installation_id, github_account_login, github_account_type, target_type)
			 VALUES (0, $1, $2, $3, 'selected') RETURNING id",
		)
		.bind(installation.id)
		.bind(&login)
		.bind(&account_type)
		.fetch_one(&state.db)
		.await
		.map_err(|error| db_error(&error))?;
		id
	} else {
		sqlx::query(
			"UPDATE installations SET github_account_login = $2, updated_at = $3 WHERE id = $1",
		)
		.bind(installation_id)
		.bind(&login)
		.bind(chrono::Utc::now().to_rfc3339())
		.execute(&state.db)
		.await
		.map_err(|error| db_error(&error))?;
		installation_id
	};

	for repository in repositories {
		if add {
			let exists: Option<i32> =
				sqlx::query_scalar("SELECT id FROM repositories WHERE github_repo_id = $1")
					.bind(repository.id)
					.fetch_optional(&state.db)
					.await
					.map_err(|error| db_error(&error))?;
			if exists.is_some() {
				sqlx::query(
					"UPDATE repositories SET installation_id = $2, github_full_name = $3, github_private = $4, updated_at = $5 WHERE github_repo_id = $1",
				)
				.bind(repository.id)
				.bind(installation_id)
				.bind(&repository.full_name)
				.bind(repository.private)
				.bind(chrono::Utc::now().to_rfc3339())
				.execute(&state.db)
				.await
				.map_err(|error| db_error(&error))?;
			} else {
				sqlx::query(
					"INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private, created_at, updated_at)
					 VALUES ($1, $2, $3, $4, $5, $5)",
				)
				.bind(installation_id)
				.bind(repository.id)
				.bind(&repository.full_name)
				.bind(repository.private)
				.bind(chrono::Utc::now().to_rfc3339())
				.execute(&state.db)
				.await
				.map_err(|error| db_error(&error))?;
			}
		} else {
			sqlx::query("DELETE FROM repositories WHERE github_repo_id = $1")
				.bind(repository.id)
				.execute(&state.db)
				.await
				.map_err(|error| db_error(&error))?;
		}
	}

	tracing::info!(
		installation = installation.id,
		add,
		"installation repositories synchronized"
	);
	Ok(StatusCode::OK)
}

async fn delete_installation_repositories(
	state: &AppState,
	github_installation_id: i64,
) -> Result<StatusCode, (StatusCode, Json<WebhookError>)> {
	sqlx::query("DELETE FROM installations WHERE github_installation_id = $1")
		.bind(github_installation_id)
		.execute(&state.db)
		.await
		.map_err(|error| db_error(&error))?;
	// repositories cascade through installation_id
	tracing::info!(
		installation = github_installation_id,
		"installation deleted"
	);
	Ok(StatusCode::OK)
}

async fn set_installation_suspended(
	state: &AppState,
	github_installation_id: i64,
	suspended: bool,
) -> Result<StatusCode, (StatusCode, Json<WebhookError>)> {
	let target_type = if suspended { "suspended" } else { "selected" };
	sqlx::query(
		"UPDATE installations SET target_type = $2, updated_at = $3 WHERE github_installation_id = $1",
	)
	.bind(github_installation_id)
	.bind(target_type)
	.bind(chrono::Utc::now().to_rfc3339())
	.execute(&state.db)
	.await
	.map_err(|error| db_error(&error))?;
	tracing::info!(
		installation = github_installation_id,
		suspended,
		"installation suspension updated"
	);
	Ok(StatusCode::OK)
}

fn db_error(error: &sqlx::Error) -> (StatusCode, Json<WebhookError>) {
	tracing::error!(%error, "webhook database error");
	(
		StatusCode::INTERNAL_SERVER_ERROR,
		Json(WebhookError {
			message: "database error".to_string(),
		}),
	)
}
