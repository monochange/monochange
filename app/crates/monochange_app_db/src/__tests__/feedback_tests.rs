//! Feedback documents save with optimistic concurrency.

#![allow(clippy::disallowed_methods)]

use super::NotificationRecord;
use super::SaveOutcome;
use super::StoredFeedback;
use super::list_notifications;
use super::load_feedback;
use super::save_feedback;
use crate::DbPool;

async fn pool() -> DbPool {
	let pool = crate::create_pool("sqlite::memory:").await.unwrap();
	crate::run_migrations(&pool).await.unwrap();
	sqlx::query("INSERT INTO organizations (id, github_id, github_login) VALUES (1, 5001, 'acme')")
		.execute(&pool)
		.await
		.unwrap();
	sqlx::query(
		"INSERT INTO projects (id, organization_id, slug, name) VALUES (1, 1, 'invoices', 'Invoices')",
	)
	.execute(&pool)
	.await
	.unwrap();
	pool
}

fn notification(recipient: &str, body: &str) -> NotificationRecord {
	NotificationRecord {
		recipient: recipient.to_owned(),
		item_id: "fb-1".to_owned(),
		notification_json: body.to_owned(),
	}
}

#[tokio::test]
async fn documents_are_created_then_updated_by_version() {
	let pool = pool().await;
	assert_eq!(load_feedback(&pool, 1).await.unwrap(), None);
	assert_eq!(
		save_feedback(&pool, 1, "{\"v\":1}", None, &[])
			.await
			.unwrap(),
		SaveOutcome::Saved { version: 1 }
	);
	assert_eq!(
		save_feedback(
			&pool,
			1,
			"{\"v\":2}",
			Some(1),
			&[notification("anon-1", "first")]
		)
		.await
		.unwrap(),
		SaveOutcome::Saved { version: 2 }
	);
	assert_eq!(
		load_feedback(&pool, 1).await.unwrap(),
		Some(StoredFeedback {
			state_json: "{\"v\":2}".to_owned(),
			version: 2,
		})
	);
}

#[tokio::test]
async fn stale_writers_conflict_and_store_nothing() {
	let pool = pool().await;
	save_feedback(&pool, 1, "{}", None, &[]).await.unwrap();
	// A second creator and a writer holding an old version both lose.
	assert_eq!(
		save_feedback(
			&pool,
			1,
			"{\"other\":true}",
			None,
			&[notification("a", "x")]
		)
		.await
		.unwrap(),
		SaveOutcome::Conflict
	);
	assert_eq!(
		save_feedback(
			&pool,
			1,
			"{\"stale\":true}",
			Some(7),
			&[notification("a", "x")]
		)
		.await
		.unwrap(),
		SaveOutcome::Conflict
	);
	assert_eq!(
		load_feedback(&pool, 1).await.unwrap().unwrap().state_json,
		"{}"
	);
	assert!(
		list_notifications(&pool, 1, "a", 10)
			.await
			.unwrap()
			.is_empty()
	);
}

#[tokio::test]
async fn notifications_are_listed_newest_first_per_recipient() {
	let pool = pool().await;
	save_feedback(
		&pool,
		1,
		"{}",
		None,
		&[
			notification("anon-1", "first"),
			notification("anon-2", "other"),
			notification("anon-1", "second"),
		],
	)
	.await
	.unwrap();
	assert_eq!(
		list_notifications(&pool, 1, "anon-1", 10).await.unwrap(),
		["second", "first"]
	);
	assert_eq!(
		list_notifications(&pool, 1, "anon-1", 1).await.unwrap(),
		["second"]
	);
	assert!(
		list_notifications(&pool, 1, "nobody", 10)
			.await
			.unwrap()
			.is_empty()
	);
}

#[tokio::test]
async fn database_failures_surface() {
	let pool = pool().await;
	pool.close().await;
	assert!(load_feedback(&pool, 1).await.is_err());
	assert!(save_feedback(&pool, 1, "{}", None, &[]).await.is_err());
	assert!(list_notifications(&pool, 1, "a", 1).await.is_err());
}

#[test]
fn migration_creates_and_drops_feedback_tables() {
	use welds::migrations::MigrationWriter;
	let writer = crate::CreateProjectFeedback;
	let up = writer.up_sql(welds::Syntax::Sqlite).join("\n");
	assert!(up.contains("CREATE TABLE IF NOT EXISTS project_feedback"));
	assert!(up.contains("CREATE TABLE IF NOT EXISTS feedback_notifications"));
	let down = writer.down_sql(welds::Syntax::Sqlite).join("\n");
	assert!(down.contains("DROP TABLE IF EXISTS project_feedback;"));
}
