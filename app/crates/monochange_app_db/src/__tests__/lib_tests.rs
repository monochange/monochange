//! Unit tests for database models, migrations, and error types.

// `#[tokio::test]` generates `Builder::expect("Failed building the Runtime")`
// internally; the app code below never calls `Result::expect` directly.
#![allow(clippy::disallowed_methods)]

use rstest::rstest;
use welds::state::DbState;

use crate::models::*;

// ── Model construction tests ──
#[rstest]
fn test_user_new_has_defaults() {
	let user = User::new();
	assert_eq!(user.id, 0);
	assert_eq!(user.github_id, 0);
	assert_eq!(user.github_login, String::new());
}

#[rstest]
fn test_installation_new() {
	let inst = Installation::new();
	assert_eq!(inst.github_installation_id, 0);
	assert_eq!(inst.target_type, String::new());
}

#[rstest]
fn test_repository_new() {
	let repo = Repository::new();
	assert!(!repo.github_private);
	assert_eq!(repo.plan_tier, String::new());
}

#[rstest]
fn test_organization_new() {
	let org = Organization::new();
	assert_eq!(org.github_id, 0);
	assert_eq!(org.github_login, String::new());
}

#[rstest]
fn test_org_member_new() {
	let member = OrganizationMember::new();
	assert_eq!(member.role, String::new());
	assert_eq!(member.user_id, 0);
	assert_eq!(member.org_id, 0);
}

// ── Migration tests ──
#[rstest]
fn test_migration_writer_up_sql() {
	use welds::migrations::MigrationWriter;
	let writer = crate::CreateUsersTable;
	let sql = writer.up_sql(welds::Syntax::Sqlite);
	assert!(!sql.is_empty());
	let combined = sql.join("\n");
	assert!(combined.contains("CREATE TABLE IF NOT EXISTS users"));
	assert!(combined.contains("CREATE TABLE IF NOT EXISTS installations"));
	assert!(combined.contains("CREATE TABLE IF NOT EXISTS repositories"));
	assert!(combined.contains("organization_members"));

	assert!(combined.contains("organizations"));
}

#[rstest]
fn test_migration_writer_down_sql() {
	use welds::migrations::MigrationWriter;
	let writer = crate::CreateUsersTable;
	let sql = writer.down_sql(welds::Syntax::Sqlite);
	let combined = sql.join("\n");
	assert!(combined.contains("DROP TABLE IF EXISTS users"));
	assert!(combined.contains("DROP TABLE IF EXISTS installations"));
	assert!(combined.contains("DROP TABLE IF EXISTS repositories"));
}

#[rstest]
fn test_release_automation_migration_writer_up_sql() {
	use welds::migrations::MigrationWriter;
	let writer = crate::CreateReleaseAutomationTables;
	let sql = writer.up_sql(welds::Syntax::Sqlite);
	let combined = sql.join("\n");
	assert!(combined.contains("CREATE TABLE IF NOT EXISTS release_schedules"));
	assert!(combined.contains("CREATE TABLE IF NOT EXISTS release_jobs"));
	assert!(combined.contains("idempotency_key TEXT NOT NULL UNIQUE"));
}

#[rstest]
fn test_release_automation_migration_writer_down_sql() {
	use welds::migrations::MigrationWriter;
	let writer = crate::CreateReleaseAutomationTables;
	let sql = writer.down_sql(welds::Syntax::Sqlite);
	let combined = sql.join("\n");
	assert!(combined.contains("DROP TABLE IF EXISTS release_jobs"));
	assert!(combined.contains("DROP TABLE IF EXISTS release_schedules"));
}

#[rstest]
fn test_github_user_token_refresh_migration_writer() {
	use welds::migrations::MigrationWriter;
	let writer = crate::AddGitHubUserTokenRefresh;
	let up = writer.up_sql(welds::Syntax::Sqlite).join("\n");
	assert!(up.contains("github_refresh_token TEXT"));
	assert!(up.contains("github_access_token_expires_at INTEGER"));
	assert!(up.contains("github_refresh_token_expires_at INTEGER"));
	let down = writer.down_sql(welds::Syntax::Sqlite).join("\n");
	assert!(down.contains("DROP COLUMN github_refresh_token"));
}

#[rstest]
fn test_migration_writers_emit_one_statement_per_entry() {
	use welds::migrations::MigrationWriter;

	let statements = [
		crate::CreateUsersTable.up_sql(welds::Syntax::Sqlite),
		crate::CreateUsersTable.down_sql(welds::Syntax::Sqlite),
		crate::CreateReleaseAutomationTables.up_sql(welds::Syntax::Sqlite),
		crate::CreateReleaseAutomationTables.down_sql(welds::Syntax::Sqlite),
		crate::AddGitHubUserTokenRefresh.up_sql(welds::Syntax::Sqlite),
		crate::AddGitHubUserTokenRefresh.down_sql(welds::Syntax::Sqlite),
	]
	.concat();

	assert!(!statements.is_empty());

	for statement in statements {
		assert!(
			statement.trim_end().ends_with(';'),
			"statement should keep its terminator: {statement}",
		);
		assert_eq!(
			statement.matches(';').count(),
			1,
			"statement should be safe for prepared execution: {statement}",
		);
	}
}

#[tokio::test]
async fn sqlite_migrations_add_expiring_github_user_token_columns() {
	let pool = crate::create_pool("sqlite::memory:")
		.await
		.unwrap_or_else(|error| panic!("create sqlite pool: {error}"));
	crate::run_migrations(&pool)
		.await
		.unwrap_or_else(|error| panic!("run sqlite migrations: {error}"));
	let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('users')")
		.fetch_all(&pool)
		.await
		.unwrap_or_else(|error| panic!("load users columns: {error}"));
	for column in [
		"github_refresh_token",
		"github_access_token_expires_at",
		"github_refresh_token_expires_at",
	] {
		assert!(columns.contains(&column.to_string()), "missing {column}");
	}
}

#[rstest]
fn test_up_sql_includes_indexes() {
	use welds::migrations::MigrationWriter;
	let user_writer = crate::CreateUsersTable;
	let automation_writer = crate::CreateReleaseAutomationTables;
	let combined = [
		user_writer.up_sql(welds::Syntax::Sqlite),
		automation_writer.up_sql(welds::Syntax::Sqlite),
	]
	.concat()
	.join("\n");
	assert!(combined.contains("CREATE INDEX IF NOT EXISTS"));
	assert!(combined.contains("idx_release_jobs_due"));
}

// ── SQLite integration tests ──
#[tokio::test]
async fn sqlite_migrations_create_expected_timestamp_columns() {
	let pool = crate::create_pool("sqlite::memory:")
		.await
		.unwrap_or_else(|error| panic!("create sqlite pool: {error}"));
	crate::run_migrations(&pool)
		.await
		.unwrap_or_else(|error| panic!("run sqlite migrations: {error}"));

	for table in [
		"users",
		"organizations",
		"organization_members",
		"installations",
		"repositories",
		"release_schedules",
		"release_jobs",
	] {
		let columns: Vec<String> =
			sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
				.fetch_all(&pool)
				.await
				.unwrap_or_else(|error| panic!("load columns for {table}: {error}"));
		assert!(
			columns.contains(&"created_at".to_string()),
			"{table} should have created_at"
		);
		assert!(
			columns.contains(&"updated_at".to_string()),
			"{table} should have updated_at"
		);
	}
}

#[tokio::test]
async fn welds_timestamp_hooks_set_created_and_updated_at() {
	let pool = crate::create_pool("sqlite::memory:")
		.await
		.unwrap_or_else(|error| panic!("create sqlite pool: {error}"));
	crate::run_migrations(&pool)
		.await
		.unwrap_or_else(|error| panic!("run sqlite migrations: {error}"));
	let client =
		crate::get_client(&pool).unwrap_or_else(|error| panic!("create welds client: {error}"));

	let stale = chrono::DateTime::from_timestamp(0, 0).unwrap_or_else(|| panic!("valid timestamp"));
	let mut user = DbState::new_uncreated(
		User::builder()
			.github_id(42)
			.github_login("octocat")
			.github_access_token("token")
			.created_at(stale)
			.updated_at(stale)
			.build(),
	);
	user.save(&client)
		.await
		.unwrap_or_else(|error| panic!("save user: {error}"));
	assert!(user.created_at > stale);
	assert_eq!(user.created_at, user.updated_at);

	let created_at = user.created_at;
	user.github_login = "octocat-renamed".to_string();
	user.save(&client)
		.await
		.unwrap_or_else(|error| panic!("update user: {error}"));
	assert_eq!(user.created_at, created_at);
	assert!(user.updated_at >= created_at);
}

// ── Error type tests ──
#[rstest]
fn test_db_error_connection_display() {
	let err = crate::DbError::Connection("timeout".into());
	assert_eq!(err.to_string(), "Connection failed: timeout");
}

#[rstest]
fn test_db_error_migration_display() {
	let err = crate::DbError::Migration("invalid sql".into());
	assert_eq!(err.to_string(), "Migration failed: invalid sql");
}

#[rstest]
fn test_db_error_debug() {
	let err = crate::DbError::Connection("test".into());
	let debug = format!("{err:?}");
	assert!(debug.contains("Connection"));
}

#[rstest]
fn organizations_and_projects_migration_is_reversible_where_sqlite_allows() {
	use welds::migrations::MigrationWriter;
	let writer = crate::AddOrganizationsAndProjects;
	let up = writer.up_sql(welds::Syntax::Sqlite).join("\n");
	assert!(up.contains("CREATE TABLE IF NOT EXISTS projects"));
	assert!(up.contains("CREATE TABLE IF NOT EXISTS project_repositories"));
	assert!(up.contains("ALTER TABLE installations ADD COLUMN organization_id"));
	let down = writer.down_sql(welds::Syntax::Sqlite).join("\n");
	assert!(down.contains("DROP TABLE IF EXISTS projects;"));
	assert!(down.contains("ALTER TABLE users DROP COLUMN provider;"));

	// SQLite cannot drop a foreign-key column, so rollback leaves it inert.
	assert!(!down.contains("DROP COLUMN organization_id"));
}

// ── Release-upgrade rehearsal (v0.3.3 → candidate) ──
//
// Proves the deployed upgrade path on a real file database: an authentic
// v0.3.3 database (the real first-five migration implementation, seeded from
// checked-in text fixtures), upgraded through the current runner, snapshotted
// and restored through the deployed `sqlite3 .backup` mechanism (rehearsed by
// scripts/sqlite-backup-rehearsal.ts against the same schema fixtures).

fn release_upgrade_fixture(name: &str) -> std::path::PathBuf {
	std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../../fixtures/tests/app-sqlite-release-upgrade/")
		.join(name)
}

fn release_upgrade_fixture_content(name: &str) -> String {
	std::fs::read_to_string(release_upgrade_fixture(name))
		.unwrap_or_else(|error| panic!("read fixture {name}: {error}"))
}

/// Exactly the migration list v0.3.3's `run_migrations` passed to welds: the
/// tag's implementation of these five writers is byte-identical to this
/// crate's (verified against `monochange_app/v0.3.3`), so running them here
/// builds an authentic legacy database.
fn legacy_v033_migrations() -> &'static [welds::migrations::MigrationFn] {
	&[
		crate::create_users_tables,
		crate::create_release_automation_tables,
		crate::add_github_user_token_refresh,
		crate::add_organizations_and_projects,
		crate::create_project_feedback,
	]
}

async fn legacy_pool() -> sqlx::SqlitePool {
	let pool = crate::create_pool("sqlite::memory:")
		.await
		.unwrap_or_else(|error| panic!("create sqlite pool: {error}"));
	let client =
		crate::get_client(&pool).unwrap_or_else(|error| panic!("create welds client: {error}"));
	welds::migrations::up(&client, legacy_v033_migrations())
		.await
		.unwrap_or_else(|error| panic!("run v0.3.3 migrations: {error}"));
	sqlx::raw_sql(&release_upgrade_fixture_content("seed.sql"))
		.execute(&pool)
		.await
		.unwrap_or_else(|error| panic!("seed legacy database: {error}"));
	pool
}

async fn upgraded_pool() -> sqlx::SqlitePool {
	let pool = legacy_pool().await;
	crate::run_migrations(&pool)
		.await
		.unwrap_or_else(|error| panic!("upgrade legacy database: {error}"));
	pool
}

/// Column lists of every legacy table, captured from the live schema so the
/// digest compares like with like across schema changes.
async fn legacy_table_columns(pool: &sqlx::SqlitePool) -> Vec<(String, Vec<String>)> {
	use sqlx::Row;

	let tables = [
		"feedback_notifications",
		"installations",
		"organization_members",
		"organizations",
		"project_feedback",
		"project_repositories",
		"projects",
		"release_jobs",
		"release_schedules",
		"repositories",
		"users",
	];
	let mut captured = Vec::with_capacity(tables.len());
	for table in tables {
		let columns: Vec<String> =
			sqlx::query(&format!("SELECT name FROM pragma_table_info('{table}')"))
				.fetch_all(pool)
				.await
				.unwrap_or_else(|error| panic!("read columns of {table}: {error}"))
				.iter()
				.map(|row| {
					row.try_get::<String, _>(0)
						.unwrap_or_else(|error| panic!("column name: {error}"))
				})
				.collect();
		captured.push(((*table).to_string(), columns));
	}
	captured
}

/// Every legacy table's complete content as a SHA-256 digest. Cells are
/// hex-encoded in SQL, so opaque token bytes participate without any UTF-8
/// assumption; the column lists come from the pre-upgrade snapshot.
async fn legacy_state_digest(pool: &sqlx::SqlitePool, tables: &[(String, Vec<String>)]) -> String {
	use sha2::Digest;
	use sqlx::Row;

	let mut material = String::new();
	for (table, columns) in tables {
		let projection: Vec<String> = columns
			.iter()
			.map(|column| format!("hex(\"{column}\")"))
			.collect();
		let rows = sqlx::query(&format!(
			"SELECT {} FROM \"{table}\" ORDER BY rowid",
			projection.join(", ")
		))
		.fetch_all(pool)
		.await
		.unwrap_or_else(|error| panic!("read rows of {table}: {error}"));
		material.push_str(table);
		material.push('\n');
		for row in rows {
			for index in 0..columns.len() {
				let cell: String = row
					.try_get(index)
					.unwrap_or_else(|error| panic!("hex cell {table}[{index}]: {error}"));
				material.push_str(&cell);
				material.push(',');
			}
			material.push('\n');
		}
	}
	let digest = sha2::Sha256::digest(material.as_bytes());
	hex::encode(digest)
}

/// Migration log rows as (name, `when_applied`), in application order.
async fn migration_log(pool: &sqlx::SqlitePool) -> Vec<(String, i64)> {
	use sqlx::Row;
	sqlx::query("SELECT name, when_applied FROM _welds_migrations ORDER BY id")
		.fetch_all(pool)
		.await
		.unwrap_or_else(|error| panic!("read migration log: {error}"))
		.iter()
		.map(|row| {
			let name = row
				.try_get::<String, _>(0)
				.unwrap_or_else(|error| panic!("name: {error}"));
			let when = row
				.try_get::<i64, _>(1)
				.unwrap_or_else(|error| panic!("when_applied: {error}"));
			(name, when)
		})
		.collect()
}

/// The rehearsal's record of every legacy table's expected row counts.
async fn assert_legacy_row_counts(pool: &sqlx::SqlitePool) {
	for (table, expected) in [
		("users", 3),
		("organizations", 2),
		("organization_members", 3),
		("installations", 2),
		("repositories", 3),
		("release_schedules", 1),
		("release_jobs", 2),
		("projects", 2),
		("project_repositories", 2),
		("project_feedback", 1),
		("feedback_notifications", 2),
	] {
		let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
			.fetch_one(pool)
			.await
			.unwrap_or_else(|error| panic!("count {table}: {error}"));
		assert_eq!(count, expected, "{table} row count");
	}
}

/// Representative legacy values, including the opaque token bytes, that must
/// survive the upgrade byte for byte.
async fn assert_representative_legacy_values(pool: &sqlx::SqlitePool) {
	use sqlx::Row;

	let user = sqlx::query(
		"SELECT github_login, plan_tier, hex(github_access_token), hex(github_refresh_token),
		 github_access_token_expires_at, provider, session_epoch
		 FROM users WHERE id = 2",
	)
	.fetch_one(pool)
	.await
	.unwrap_or_else(|error| panic!("read user 2: {error}"));
	assert_eq!(
		user.try_get::<String, _>(0)
			.unwrap_or_else(|error| panic!("login: {error}")),
		"fixture-release-lead"
	);
	assert_eq!(
		user.try_get::<String, _>(1)
			.unwrap_or_else(|error| panic!("plan: {error}")),
		"pro"
	);
	assert_eq!(
		user.try_get::<String, _>(2)
			.unwrap_or_else(|error| panic!("token: {error}")),
		"3D84F6E21B09AC57D0C83E661F224B558890",
		"opaque access-token bytes must survive unchanged",
	);
	assert_eq!(
		user.try_get::<String, _>(3)
			.unwrap_or_else(|error| panic!("refresh: {error}")),
		"7A55C9E301D46B289FC0E7312A6D4599",
	);
	assert_eq!(
		user.try_get::<i64, _>(4)
			.unwrap_or_else(|error| panic!("expires: {error}")),
		1_791_000_000
	);
	assert_eq!(
		user.try_get::<String, _>(5)
			.unwrap_or_else(|error| panic!("provider: {error}")),
		"github"
	);

	let repo = sqlx::query(
		"SELECT monochange_config_hash, settings_json, plan_tier, github_private
		 FROM repositories WHERE id = 1",
	)
	.fetch_one(pool)
	.await
	.unwrap_or_else(|error| panic!("read repository 1: {error}"));
	assert_eq!(
		repo.try_get::<String, _>(0)
			.unwrap_or_else(|error| panic!("hash: {error}")),
		"fixture-config-hash-4f1e2d"
	);
	assert_eq!(
		repo.try_get::<String, _>(1)
			.unwrap_or_else(|error| panic!("settings: {error}")),
		"{\"releaseBranch\":\"trunk\"}",
	);
	assert_eq!(
		repo.try_get::<i64, _>(3)
			.unwrap_or_else(|error| panic!("private: {error}")),
		1
	);

	let schedule = sqlx::query(
		"SELECT cadence_json, window_batch_index, base_ref, requested_by_user_id
		 FROM release_schedules WHERE id = 1",
	)
	.fetch_one(pool)
	.await
	.unwrap_or_else(|error| panic!("read schedule 1: {error}"));
	assert_eq!(
		schedule
			.try_get::<String, _>(0)
			.unwrap_or_else(|error| panic!("cadence: {error}")),
		"{\"kind\":\"weekly\",\"day\":\"tuesday\"}",
	);
	assert_eq!(
		schedule
			.try_get::<i64, _>(1)
			.unwrap_or_else(|error| panic!("batch: {error}")),
		2
	);
	assert_eq!(
		schedule
			.try_get::<String, _>(2)
			.unwrap_or_else(|error| panic!("base: {error}")),
		"trunk"
	);
	assert_eq!(
		schedule
			.try_get::<i64, _>(3)
			.unwrap_or_else(|error| panic!("requester: {error}")),
		1
	);

	let feedback =
		sqlx::query("SELECT state_json, version FROM project_feedback WHERE project_id = 1")
			.fetch_one(pool)
			.await
			.unwrap_or_else(|error| panic!("read feedback 1: {error}"));
	assert_eq!(
		feedback
			.try_get::<i64, _>(1)
			.unwrap_or_else(|error| panic!("version: {error}")),
		3
	);
	assert!(
		feedback
			.try_get::<String, _>(0)
			.unwrap_or_else(|error| panic!("state: {error}"))
			.contains("fixture-item-1")
	);

	// The upgraded schema must still serve every column a v0.3.3 reader names
	// (equivalent reader SQL from the tag's model definitions, in a fixture).
	for (index, statement) in release_upgrade_fixture_content("legacy-reader-checks.sql")
		.split(";\n")
		.map(str::trim)
		.filter(|statement| !statement.is_empty())
		.enumerate()
	{
		let rows = sqlx::query(statement)
			.fetch_all(pool)
			.await
			.unwrap_or_else(|error| panic!("legacy reader query {index}: {error}"));
		assert!(
			!rows.is_empty(),
			"legacy reader query {index} returned no rows"
		);
	}
}

async fn assert_database_integrity(pool: &sqlx::SqlitePool) {
	let quick_check: String = sqlx::query_scalar("PRAGMA quick_check")
		.fetch_one(pool)
		.await
		.unwrap_or_else(|error| panic!("quick_check: {error}"));
	assert_eq!(quick_check, "ok");
	let foreign_key_violations = sqlx::query("PRAGMA foreign_key_check")
		.fetch_all(pool)
		.await
		.unwrap_or_else(|error| panic!("foreign_key_check: {error}"));
	assert!(
		foreign_key_violations.is_empty(),
		"foreign_key_check must be clean"
	);
}

#[rstest]
fn release_upgrade_schema_fixtures_match_the_real_migration_writers() {
	let legacy_writers: Vec<&dyn welds::migrations::MigrationWriter> = vec![
		&crate::CreateUsersTable,
		&crate::CreateReleaseAutomationTables,
		&crate::AddGitHubUserTokenRefresh,
		&crate::AddOrganizationsAndProjects,
		&crate::CreateProjectFeedback,
	];
	let legacy = legacy_writers
		.iter()
		.flat_map(|writer| writer.up_sql(welds::Syntax::Sqlite))
		.collect::<Vec<_>>()
		.join("\n");
	assert_eq!(
		release_upgrade_fixture_content("legacy-schema.sql").trim_end_matches('\n'),
		legacy,
		"legacy-schema.sql must stay byte-equal to the real first-five migration SQL",
	);

	let candidate_writers: Vec<&dyn welds::migrations::MigrationWriter> =
		vec![&crate::CreateOAuthStateUses, &crate::CreateClientSessions];
	let candidate = candidate_writers
		.iter()
		.flat_map(|writer| writer.up_sql(welds::Syntax::Sqlite))
		.collect::<Vec<_>>()
		.join("\n");
	assert_eq!(
		release_upgrade_fixture_content("candidate-schema.sql").trim_end_matches('\n'),
		candidate,
		"candidate-schema.sql must stay byte-equal to the real 006/007 migration SQL",
	);
}

#[tokio::test]
async fn release_upgrade_preserves_v033_data_and_records_the_candidate_migrations() {
	let pool = legacy_pool().await;
	let legacy_columns = legacy_table_columns(&pool).await;
	let digest_before = legacy_state_digest(&pool, &legacy_columns).await;
	let legacy_log = migration_log(&pool).await;
	assert_eq!(
		legacy_log.len(),
		5,
		"v0.3.3 records exactly its five migrations"
	);
	assert_eq!(
		legacy_log
			.iter()
			.map(|(name, _)| name.as_str())
			.collect::<Vec<_>>(),
		[
			"001_create_users",
			"002_create_release_automation",
			"003_add_github_user_token_refresh",
			"004_add_organizations_and_projects",
			"005_create_project_feedback",
		],
	);

	crate::run_migrations(&pool)
		.await
		.unwrap_or_else(|error| panic!("upgrade: {error}"));

	// The only legacy-schema change is the documented `users.session_epoch`
	// column; every other legacy table keeps its exact column set.
	let columns_after = legacy_table_columns(&pool).await;
	for (index, (table, columns_before)) in legacy_columns.iter().enumerate() {
		let (table_after, columns) = &columns_after[index];
		assert_eq!(table_after, table);
		if table == "users" {
			assert_eq!(columns.len(), columns_before.len() + 1);
			assert_eq!(&columns[..columns_before.len()], columns_before.as_slice());
			assert_eq!(columns[columns_before.len()], "session_epoch");
		} else {
			assert_eq!(
				columns, columns_before,
				"{table} column set must not change"
			);
		}
	}

	// Every legacy row survives byte for byte, including opaque token bytes.
	assert_eq!(
		legacy_state_digest(&pool, &legacy_columns).await,
		digest_before
	);
	assert_legacy_row_counts(&pool).await;
	assert_representative_legacy_values(&pool).await;
	assert_database_integrity(&pool).await;

	// The upgrade appends exactly the candidate migrations; applied legacy
	// rows keep their original ids and timestamps.
	let upgraded_log = migration_log(&pool).await;
	assert_eq!(upgraded_log.len(), 7);
	assert_eq!(legacy_log, upgraded_log[..5]);
	assert_eq!(
		upgraded_log[5..]
			.iter()
			.map(|(name, _)| name.as_str())
			.collect::<Vec<_>>(),
		["006_create_oauth_state_uses", "007_create_client_sessions"],
	);
	for name in ["006_create_oauth_state_uses", "007_create_client_sessions"] {
		let rollback: String =
			sqlx::query_scalar("SELECT rollback_sql FROM _welds_migrations WHERE name = ?")
				.bind(name)
				.fetch_one(&pool)
				.await
				.unwrap_or_else(|error| panic!("rollback sql for {name}: {error}"));
		assert!(!rollback.is_empty(), "{name} recorded its rollback SQL");
	}

	// New auth schema exists, and every existing user reads the epoch default.
	let tables: Vec<String> =
		sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
			.fetch_all(&pool)
			.await
			.unwrap_or_else(|error| panic!("list tables: {error}"));
	for table in ["oauth_state_uses", "client_grants", "client_sessions"] {
		assert!(
			tables.contains(&table.to_string()),
			"missing new table {table}"
		);
	}
	let indexes: Vec<String> = sqlx::query_scalar(
		"SELECT name FROM sqlite_master WHERE type = 'index' AND name IS NOT NULL ORDER BY name",
	)
	.fetch_all(&pool)
	.await
	.unwrap_or_else(|error| panic!("list indexes: {error}"));
	for index in [
		"idx_oauth_state_uses_expiry",
		"idx_client_grants_user",
		"idx_client_grants_expiry",
		"idx_client_sessions_user",
		"idx_client_sessions_expiry",
		"idx_client_sessions_token",
	] {
		assert!(
			indexes.contains(&index.to_string()),
			"missing new index {index}"
		);
	}
	let epochs: Vec<i64> = sqlx::query_scalar("SELECT session_epoch FROM users ORDER BY id")
		.fetch_all(&pool)
		.await
		.unwrap_or_else(|error| panic!("read epochs: {error}"));
	assert_eq!(
		epochs,
		[0, 0, 0],
		"existing users must read the session_epoch default"
	);
}

#[tokio::test]
async fn second_upgrade_pass_is_idempotent_and_preserves_post_upgrade_writes() {
	let pool = upgraded_pool().await;
	let upgraded_columns = legacy_table_columns(&pool).await;
	let log_after_upgrade = migration_log(&pool).await;
	assert_legacy_row_counts(&pool).await;

	// First writes the upgraded app performs: a new user (epoch default
	// applies), an oauth state nonce, and a hashed client session.
	sqlx::raw_sql(&release_upgrade_fixture_content("post-upgrade-writes.sql"))
		.execute(&pool)
		.await
		.unwrap_or_else(|error| panic!("post-upgrade writes: {error}"));

	// A second migration pass over the running database must change nothing:
	// compare a full digest taken after these writes with one taken after the
	// pass, so even the newest rows are proven untouched.
	let digest_before_second_pass = legacy_state_digest(&pool, &upgraded_columns).await;
	crate::run_migrations(&pool)
		.await
		.unwrap_or_else(|error| panic!("second migration pass: {error}"));

	assert_eq!(
		legacy_state_digest(&pool, &upgraded_columns).await,
		digest_before_second_pass,
	);
	assert_eq!(
		migration_log(&pool).await,
		log_after_upgrade,
		"no migration may re-apply or rewrite history"
	);
	assert_database_integrity(&pool).await;

	// The post-upgrade writes survive the second pass: nothing in the
	// migration path rolls back or overwrites newer data.
	let session: (i64, String) = sqlx::query_as(
		"SELECT session_epoch, hex(token_hash) FROM client_sessions WHERE user_id = 4",
	)
	.fetch_one(&pool)
	.await
	.unwrap_or_else(|error| panic!("read post-upgrade session: {error}"));
	assert_eq!(session.0, 0);
	assert_eq!(session.1, "AABBCCDD0011EE2233445566");
	let nonce: i64 = sqlx::query_scalar(
		"SELECT expires_at FROM oauth_state_uses WHERE nonce = X'6E6F6E636531323334353637383930'",
	)
	.fetch_one(&pool)
	.await
	.unwrap_or_else(|error| panic!("read post-upgrade nonce: {error}"));
	assert_eq!(nonce, 1_792_000_000);
	let new_user_login: String = sqlx::query_scalar("SELECT github_login FROM users WHERE id = 4")
		.fetch_one(&pool)
		.await
		.unwrap_or_else(|error| panic!("read post-upgrade user: {error}"));
	assert_eq!(new_user_login, "fixture-post-upgrade");

	// Plain INSERT without the new column still receives the epoch default.
	let inserted_epoch: i64 = sqlx::query_scalar(
		"INSERT INTO users (id, github_id, github_login, github_access_token)
		 VALUES (5, 77005, 'fixture-default-epoch', '') RETURNING session_epoch",
	)
	.fetch_one(&pool)
	.await
	.unwrap_or_else(|error| panic!("insert default-epoch user: {error}"));
	assert_eq!(
		inserted_epoch, 0,
		"session_epoch default must apply to plain inserts"
	);
}

/// Limits of image rollback, established against the upgraded schema: the
/// v0.3.3 runner's migration list is a subset of the applied log, and welds
/// no-ops every already-applied name. This proves runner semantics and an
/// additive schema — NOT that the v0.3.3 binary itself was re-run.
#[tokio::test]
async fn legacy_v033_runner_no_ops_against_the_upgraded_schema() {
	let pool = upgraded_pool().await;
	let upgraded_columns = legacy_table_columns(&pool).await;
	let digest_before = legacy_state_digest(&pool, &upgraded_columns).await;
	let log_before = migration_log(&pool).await;

	let client =
		crate::get_client(&pool).unwrap_or_else(|error| panic!("create welds client: {error}"));
	welds::migrations::up(&client, legacy_v033_migrations())
		.await
		.unwrap_or_else(|error| panic!("v0.3.3 migration list over the upgraded schema: {error}"));

	assert_eq!(
		migration_log(&pool).await,
		log_before,
		"the legacy runner must not touch the log"
	);
	assert_eq!(
		legacy_state_digest(&pool, &upgraded_columns).await,
		digest_before
	);
	assert_database_integrity(&pool).await;
}
