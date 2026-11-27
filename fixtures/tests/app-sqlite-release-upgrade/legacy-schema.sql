CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                github_id INTEGER NOT NULL UNIQUE,
                github_login TEXT NOT NULL,
                github_avatar_url TEXT,
                github_access_token TEXT NOT NULL,
                email TEXT,
                plan_tier TEXT NOT NULL DEFAULT 'free',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
CREATE TABLE IF NOT EXISTS organizations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                github_id INTEGER NOT NULL UNIQUE,
                github_login TEXT NOT NULL,
                github_avatar_url TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
CREATE TABLE IF NOT EXISTS organization_members (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                org_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                role TEXT NOT NULL DEFAULT 'member',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                UNIQUE(user_id, org_id)
            );
CREATE TABLE IF NOT EXISTS installations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                github_installation_id INTEGER NOT NULL UNIQUE,
                github_account_login TEXT NOT NULL,
                github_account_type TEXT NOT NULL,
                target_type TEXT NOT NULL DEFAULT 'selected',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
CREATE TABLE IF NOT EXISTS repositories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
                github_repo_id INTEGER NOT NULL UNIQUE,
                github_full_name TEXT NOT NULL,
                github_private INTEGER NOT NULL DEFAULT 0,
                monochange_config_hash TEXT,
                settings_json TEXT,
                plan_tier TEXT NOT NULL DEFAULT 'free',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
CREATE INDEX IF NOT EXISTS idx_repos_installation ON repositories(installation_id);
CREATE INDEX IF NOT EXISTS idx_installations_user ON installations(user_id);
CREATE INDEX IF NOT EXISTS idx_org_members_user ON organization_members(user_id);
CREATE TABLE IF NOT EXISTS release_schedules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
                enabled INTEGER NOT NULL DEFAULT 1,
                cadence_json TEXT NOT NULL,
                next_run_at TEXT NOT NULL,
                window_batch_index INTEGER NOT NULL DEFAULT 0,
                last_enqueued_at TEXT,
                base_ref TEXT NOT NULL DEFAULT 'main',
                requested_by_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
CREATE TABLE IF NOT EXISTS release_jobs (
                id TEXT PRIMARY KEY,
                schedule_id INTEGER NOT NULL REFERENCES release_schedules(id) ON DELETE CASCADE,
                repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                run_after TEXT NOT NULL,
                scheduled_for TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 5,
                locked_by TEXT,
                locked_until TEXT,
                idempotency_key TEXT NOT NULL UNIQUE,
                payload_json TEXT NOT NULL,
                result_json TEXT,
                last_error TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
CREATE INDEX IF NOT EXISTS idx_release_schedules_due ON release_schedules(enabled, next_run_at);
CREATE INDEX IF NOT EXISTS idx_release_jobs_due ON release_jobs(status, run_after);
CREATE INDEX IF NOT EXISTS idx_release_jobs_lock ON release_jobs(locked_until);
CREATE INDEX IF NOT EXISTS idx_release_jobs_repository ON release_jobs(repository_id);
ALTER TABLE users ADD COLUMN github_refresh_token TEXT;
ALTER TABLE users ADD COLUMN github_access_token_expires_at INTEGER;
ALTER TABLE users ADD COLUMN github_refresh_token_expires_at INTEGER;
ALTER TABLE users ADD COLUMN provider TEXT NOT NULL DEFAULT 'github';
ALTER TABLE organizations ADD COLUMN provider TEXT NOT NULL DEFAULT 'github';
ALTER TABLE organizations ADD COLUMN account_type TEXT NOT NULL DEFAULT 'Organization';
ALTER TABLE installations ADD COLUMN provider TEXT NOT NULL DEFAULT 'github';
ALTER TABLE installations ADD COLUMN organization_id INTEGER REFERENCES organizations(id) ON DELETE SET NULL;
ALTER TABLE repositories ADD COLUMN provider TEXT NOT NULL DEFAULT 'github';
CREATE UNIQUE INDEX IF NOT EXISTS idx_organizations_provider_account ON organizations(provider, github_id);
CREATE INDEX IF NOT EXISTS idx_installations_organization ON installations(organization_id);
CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                slug TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                created_by_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                UNIQUE(organization_id, slug)
            );
CREATE TABLE IF NOT EXISTS project_repositories (
                project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                provider TEXT NOT NULL DEFAULT 'github',
                repository_external_id INTEGER NOT NULL,
                full_name TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                PRIMARY KEY (project_id, provider, repository_external_id)
            );
CREATE INDEX IF NOT EXISTS idx_project_repositories_repository ON project_repositories(provider, repository_external_id);
CREATE TABLE IF NOT EXISTS project_feedback (
                project_id INTEGER PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
                state_json TEXT NOT NULL,
                version INTEGER NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
CREATE TABLE IF NOT EXISTS feedback_notifications (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                recipient TEXT NOT NULL,
                item_id TEXT NOT NULL,
                notification_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
CREATE INDEX IF NOT EXISTS idx_feedback_notifications_recipient ON feedback_notifications(project_id, recipient, id);
