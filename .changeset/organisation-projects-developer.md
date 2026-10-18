---
monochange_app: feat
---

# Store organisations and projects for connected repositories

Database migration `004_add_organizations_and_projects` adds `provider` to `users`, `organizations`, `installations`, and `repositories`, adds `installations.organization_id` and `organizations.account_type`, and creates `projects` and `project_repositories`. Project repositories are keyed by `(provider, repository_external_id)` instead of `repositories.id`, so projects survive an uninstall and reinstall. Rolling the migration back leaves `installations.organization_id` in place because SQLite cannot drop a foreign-key column.

`installation` and `installation_repositories` webhooks now upsert the installation's account into `organizations` through `monochange_app_db::projects::link_installation_organization`. Installations recorded earlier are linked on first visit through the new `GitHubAppAuth::installation_account`, which reads `GET /app/installations/{id}` with the app JWT.

New server functions in `monochange_app::server_fns::organizations` (`list_organizations`, `organization_overview`, `create_project`, `project_overview`, `update_project`, `delete_project`) derive access from the repositories a user can manage, reusing the existing organisation-owner check. Routes `/dashboard/{organization}` and `/dashboard/{organization}/projects/{project}` render them; project forms use plain `ActionForm` fields such as `repositories[]`, so they work before hydration.
