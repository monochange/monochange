-- Legacy reader checks: one SELECT per v0.3.3 welds model, selecting exactly
-- the columns each model read at tag monochange_app/v0.3.3
-- (app/crates/monochange_app_db/src/models/{user,installation,repository,
-- organization,release_schedule,release_job}.rs welds(rename) field lists,
-- plus organization_members via the HasMany on user.rs).
--
-- These queries are equivalent to what the old ORM generates; running them
-- proves the upgraded schema still serves every column a v0.3.3 reader names.
-- The rehearsal drives them as plain SQL; it does not run the old binary, so
-- this is reader-schema compatibility, not old-image compatibility.
SELECT id, github_id, github_login, github_avatar_url, github_access_token, email, plan_tier, created_at, updated_at, github_refresh_token, github_access_token_expires_at, github_refresh_token_expires_at FROM users ORDER BY id;
SELECT id, user_id, github_installation_id, github_account_login, github_account_type, target_type, created_at, updated_at FROM installations ORDER BY id;
SELECT id, installation_id, github_repo_id, github_full_name, github_private, monochange_config_hash, settings_json, plan_tier, created_at, updated_at FROM repositories ORDER BY id;
SELECT id, github_id, github_login, github_avatar_url, created_at, updated_at FROM organizations ORDER BY id;
SELECT id, user_id, org_id, role, created_at, updated_at FROM organization_members ORDER BY id;
SELECT id, repository_id, enabled, cadence_json, next_run_at, window_batch_index, last_enqueued_at, base_ref, requested_by_user_id, created_at, updated_at FROM release_schedules ORDER BY id;
SELECT id, schedule_id, repository_id, kind, status, run_after, scheduled_for, attempts, max_attempts, locked_by, locked_until, idempotency_key, payload_json, result_json, last_error, created_at, updated_at FROM release_jobs ORDER BY id;
