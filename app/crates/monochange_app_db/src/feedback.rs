//! Storage for each project's feedback document and its notifications.
//!
//! The document's shape belongs to the feedback crate; this module stores it
//! as JSON text with a version number so concurrent writers cannot silently
//! overwrite each other.

use crate::DbPool;

/// A project's stored feedback document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFeedback {
	pub state_json: String,
	pub version: i64,
}

/// A notification to store alongside a saved document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRecord {
	pub recipient: String,
	pub item_id: String,
	pub notification_json: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
	Saved {
		version: i64,
	},
	/// Someone else saved first; reload and try again.
	Conflict,
}

fn now() -> String {
	chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub async fn load_feedback(
	pool: &DbPool,
	project_id: i32,
) -> Result<Option<StoredFeedback>, sqlx::Error> {
	let row: Option<(String, i64)> =
		sqlx::query_as("SELECT state_json, version FROM project_feedback WHERE project_id = $1")
			.bind(project_id)
			.fetch_optional(pool)
			.await?;
	Ok(row.map(|(state_json, version)| {
		StoredFeedback {
			state_json,
			version,
		}
	}))
}

/// Saves the document if it is still at `expected_version` (`None` for a
/// project with no document yet) and stores `notifications` with it.
pub async fn save_feedback(
	pool: &DbPool,
	project_id: i32,
	state_json: &str,
	expected_version: Option<i64>,
	notifications: &[NotificationRecord],
) -> Result<SaveOutcome, sqlx::Error> {
	let mut transaction = pool.begin().await?;
	let saved = match expected_version {
		None => {
			sqlx::query(
				"INSERT INTO project_feedback (project_id, state_json, version, updated_at)
				 VALUES ($1, $2, 1, $3) ON CONFLICT(project_id) DO NOTHING",
			)
			.bind(project_id)
			.bind(state_json)
			.bind(now())
			.execute(&mut *transaction)
			.await?
			.rows_affected()
		}
		Some(version) => sqlx::query(
			"UPDATE project_feedback SET state_json = $2, version = version + 1, updated_at = $3
				 WHERE project_id = $1 AND version = $4",
		)
		.bind(project_id)
		.bind(state_json)
		.bind(now())
		.bind(version)
		.execute(&mut *transaction)
		.await?
		.rows_affected(),
	};
	if saved == 0 {
		return Ok(SaveOutcome::Conflict);
	}
	for notification in notifications {
		sqlx::query(
			"INSERT INTO feedback_notifications (project_id, recipient, item_id, notification_json, created_at)
			 VALUES ($1, $2, $3, $4, $5)",
		)
		.bind(project_id)
		.bind(&notification.recipient)
		.bind(&notification.item_id)
		.bind(&notification.notification_json)
		.bind(now())
		.execute(&mut *transaction)
		.await?;
	}
	transaction.commit().await?;
	Ok(SaveOutcome::Saved {
		version: expected_version.map_or(1, |version| version + 1),
	})
}

/// The newest notifications for one recipient, newest first.
pub async fn list_notifications(
	pool: &DbPool,
	project_id: i32,
	recipient: &str,
	limit: u32,
) -> Result<Vec<String>, sqlx::Error> {
	sqlx::query_scalar(
		"SELECT notification_json FROM feedback_notifications
		 WHERE project_id = $1 AND recipient = $2 ORDER BY id DESC LIMIT $3",
	)
	.bind(project_id)
	.bind(recipient)
	.bind(limit)
	.fetch_all(pool)
	.await
}

#[cfg(test)]
#[path = "__tests__/feedback_tests.rs"]
mod tests;
