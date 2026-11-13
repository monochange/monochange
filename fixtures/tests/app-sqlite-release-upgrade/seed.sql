-- Deliberately fake seed records for the sqlite release-upgrade rehearsal.
--
-- Every name, id, token, and URL below is invented fixture data shaped like
-- what a real v0.3.3 deployment holds; nothing here comes from production.
-- Token values are opaque byte strings (hex literals cast to TEXT) because
-- production stores sealed ciphertext bytes there since monochange_app 0.4.0
-- and arbitrary legacy bytes before it; the rehearsal only proves those bytes
-- survive the upgrade unchanged.
--
-- Written against the real first-five-migration schema (see
-- legacy-schema.sql; column shapes verified with pragma_table_info).
PRAGMA foreign_keys = ON;

-- ── users ────────────────────────────────────────────────────────────────
INSERT INTO users (id, github_id, github_login, github_avatar_url, github_access_token, email, plan_tier, created_at, updated_at, github_refresh_token, github_access_token_expires_at, github_refresh_token_expires_at, provider) VALUES
	(1, 77001, 'fixture-oct-keeper', 'https://avatars.example.com/u/77001?v=4', CAST(X'9F1C4A7B2E8D5063A1B4C7E9021354647981' AS TEXT), 'keeper@example.com', 'free', '2026-09-02T10:15:30.120Z', '2026-10-01T08:00:00.000Z', NULL, NULL, NULL, 'github'),
	(2, 77002, 'fixture-release-lead', 'https://avatars.example.com/u/77002?v=4', CAST(X'3D84F6E21B09AC57D0C83E661F224B558890' AS TEXT), 'lead@example.com', 'pro', '2026-09-10T14:30:00.000Z', '2026-10-20T19:45:10.250Z', CAST(X'7A55C9E301D46B289FC0E7312A6D4599' AS TEXT), 1791000000, 1792000000, 'github'),
	(3, 77003, 'fixture-audit-bot', NULL, CAST(X'0BE4D28C91677F03A5B8D1E44C927766' AS TEXT), NULL, 'free', '2026-09-28T06:05:00.000Z', '2026-09-28T06:05:00.000Z', NULL, NULL, NULL, 'github');

-- ── organizations and memberships ───────────────────────────────────────
INSERT INTO organizations (id, github_id, github_login, github_avatar_url, created_at, updated_at, provider, account_type) VALUES
	(1, 88001, 'fixture-labs', 'https://avatars.example.com/o/88001?v=4', '2026-09-03T09:00:00.000Z', '2026-09-03T09:00:00.000Z', 'github', 'Organization'),
	(2, 88002, 'release-craft-collective', NULL, '2026-09-15T11:20:00.000Z', '2026-09-15T11:20:00.000Z', 'github', 'Organization');

INSERT INTO organization_members (id, user_id, org_id, role, created_at, updated_at) VALUES
	(1, 1, 1, 'admin', '2026-09-03T09:01:00.000Z', '2026-09-03T09:01:00.000Z'),
	(2, 2, 1, 'member', '2026-09-11T10:00:00.000Z', '2026-09-11T10:00:00.000Z'),
	(3, 3, 2, 'member', '2026-09-28T06:10:00.000Z', '2026-09-28T06:10:00.000Z');

-- ── installations and repositories ──────────────────────────────────────
INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type, target_type, created_at, updated_at, provider, organization_id) VALUES
	(1, 1, 990011, 'fixture-labs', 'Organization', 'selected', '2026-09-03T09:05:00.000Z', '2026-09-03T09:05:00.000Z', 'github', 1),
	(2, 2, 990012, 'release-craft-collective', 'Organization', 'selected', '2026-09-16T08:40:00.000Z', '2026-09-16T08:40:00.000Z', 'github', 2);

INSERT INTO repositories (id, installation_id, github_repo_id, github_full_name, github_private, monochange_config_hash, settings_json, plan_tier, created_at, updated_at, provider) VALUES
	(1, 1, 1122001, 'fixture-labs/fixture-console', 1, 'fixture-config-hash-4f1e2d', '{"releaseBranch":"trunk"}', 'pro', '2026-09-03T09:06:00.000Z', '2026-10-12T12:00:00.000Z', 'github'),
	(2, 1, 1122002, 'fixture-labs/fixture-docs', 0, NULL, NULL, 'free', '2026-09-05T16:30:00.000Z', '2026-09-05T16:30:00.000Z', 'github'),
	(3, 2, 1122003, 'release-craft-collective/craft-workflow', 0, 'fixture-config-hash-9a8b7c', '{"cadence":"weekly"}', 'free', '2026-09-16T08:41:00.000Z', '2026-09-16T08:41:00.000Z', 'github');

-- ── release automation state ────────────────────────────────────────────
INSERT INTO release_schedules (id, repository_id, enabled, cadence_json, next_run_at, window_batch_index, last_enqueued_at, base_ref, requested_by_user_id, created_at, updated_at) VALUES
	(1, 1, 1, '{"kind":"weekly","day":"tuesday"}', '2026-11-17T09:00:00.000Z', 2, '2026-11-10T09:00:00.000Z', 'trunk', 1, '2026-09-20T10:00:00.000Z', '2026-11-10T09:00:05.000Z');

INSERT INTO release_jobs (id, schedule_id, repository_id, kind, status, run_after, scheduled_for, attempts, max_attempts, locked_by, locked_until, idempotency_key, payload_json, result_json, last_error, created_at, updated_at) VALUES
	('fixture-job-completed-01', 1, 1, 'release_dispatch', 'completed', '2026-11-10T09:00:00.000Z', '2026-11-10T09:00:00.000Z', 1, 5, NULL, NULL, 'fixture-idem-completed-01', '{"windowBatchIndex":2}', '{"dispatched":true}', NULL, '2026-11-10T08:55:00.000Z', '2026-11-10T09:01:00.000Z'),
	('fixture-job-queued-02', 1, 1, 'release_dispatch', 'queued', '2026-11-17T09:00:00.000Z', '2026-11-17T09:00:00.000Z', 0, 5, NULL, NULL, 'fixture-idem-queued-02', '{"windowBatchIndex":0}', NULL, NULL, '2026-11-10T09:01:10.000Z', '2026-11-10T09:01:10.000Z');

-- ── projects, repository attachments, feedback ──────────────────────────
INSERT INTO projects (id, organization_id, slug, name, description, created_by_user_id, created_at, updated_at) VALUES
	(1, 1, 'fixture-console', 'Fixture Console', 'Example project used by the upgrade rehearsal', 1, '2026-09-21T12:00:00.000Z', '2026-10-01T09:30:00.000Z'),
	(2, 2, 'craft-workflow', 'Craft Workflow', 'Second fixture project for the collective', 2, '2026-09-30T15:00:00.000Z', '2026-09-30T15:00:00.000Z');

INSERT INTO project_repositories (project_id, provider, repository_external_id, full_name, created_at) VALUES
	(1, 'github', 1122001, 'fixture-labs/fixture-console', '2026-09-21T12:01:00.000Z'),
	(2, 'github', 1122003, 'release-craft-collective/craft-workflow', '2026-09-30T15:01:00.000Z');

INSERT INTO project_feedback (project_id, state_json, version, updated_at) VALUES
	(1, '{"items":[{"id":"fixture-item-1","author":"fixture-oct-keeper","body":"Example feedback body"}],"subscribers":["keeper@example.com"]}', 3, '2026-10-12T12:00:00.000Z');

INSERT INTO feedback_notifications (id, project_id, recipient, item_id, notification_json, created_at) VALUES
	(1, 1, 'keeper@example.com', 'fixture-item-1', '{"kind":"new_item","itemId":"fixture-item-1"}', '2026-10-12T12:00:01.000Z'),
	(2, 1, 'lead@example.com', 'fixture-item-1', '{"kind":"new_item","itemId":"fixture-item-1"}', '2026-10-12T12:00:02.000Z');
