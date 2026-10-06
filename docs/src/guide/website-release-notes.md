# Put release notes in your website or app

<!-- {=websiteReleaseNotes} -->

The website at [monochange.dev](https://monochange.dev/changelog) uses monochange to version itself and display user-facing release notes. Its version is independent of the CLI. A website release never publishes a Cargo package.

## Register a website release target

The application keeps its version in `app/crates/monochange_app/Cargo.toml`. Register that private Cargo package with a namespaced release identity:

```toml
[package.monochange_app]
path = "app/crates/monochange_app"
additional_paths = ["app/**", "Dockerfile", ".github/workflows/app-deploy.yml"]
publish = { enabled = false }
changelog = false
tag = true
release = true
version_format = "namespaced"
release_title = "monochange.dev {{ version }}"
changelog_version_title = "{{ version }}"
ignore_ecosystem_versioned_files = true
```

`changelog = false` disables the implicit developer-facing file for this target. The named outputs below own its website history. `publish.enabled = false` keeps the application out of registry publication while retaining versioning and release records. `ignore_ecosystem_versioned_files = true` avoids stamping the repository's root Cargo manifest with the website version. The Cargo adapter still updates the app's native manifest and lockfile.

## Write for website users

Types choose the audience stream. Declare a website stream and types for features, fixes, and changes requiring user action:

```toml
[changelog.streams.website]
description = "User-facing updates for monochange.dev"

[changelog.types]
website_feature = { bump = "minor", section = "website_added", stream = "website" }
website_fix = { bump = "patch", section = "website_fixed", stream = "website" }
website_change = { bump = "major", section = "website_changed", stream = "website" }

[changelog.sections]
website_added = { heading = "New", priority = 20 }
website_fixed = { heading = "Improved", priority = 30 }
website_changed = { heading = "Changed", priority = 10 }
```

Each changeset describes an outcome that visitors can recognize:

```markdown
---
monochange_app: website_feature
---

# Follow what's new on monochange.dev

Read website updates on the What's new page, with versions separate from the CLI.
```

Keep API and implementation notes in a separate changeset using the default stream. Do not combine audiences in one file. Bumps follow the repository's SemVer policy, including its policy for versions below 1.0.

## Generate Markdown and JSON from the same notes

Create one cumulative Markdown output and one immutable JSON file per website version:

```toml
[changelog.outputs.website]
stream = "website"
targets = ["monochange_app"]
path = "app/changelog.md"
format = "keep_a_changelog"
mode = "append"
initial_header = "# What's new on monochange.dev\n\nUser-facing website release notes.\n"

[changelog.outputs.website_json]
stream = "website"
targets = ["monochange_app"]
path = "app/public/releases/{{ version }}.json"
format = "json"
mode = "release"
```

Change section headings, priorities, or Markdown entry templates in `monochange.toml`. JSON keeps structured entry fields such as `summary`, `details_markdown`, `stream`, and provenance. Markdown templates do not turn those fields into Markdown strings.

Preview the named output before preparing a release:

```bash
monochange step validate
monochange preview --format json
monochange notes --output website_json --target monochange_app
```

Inspect `output`, `stream`, `owner_id`, and `path` in the dry-run artifacts. The notes command renders prospective notes without changing package versions or consuming changesets. Release preparation commits the JSON files with the version bump and the durable release record.

## Render the notes in your product

The website's `get_website_releases` server function reads the JSON artifacts shipped in its application image. It validates version filenames and stream identity, sorts versions with SemVer, and converts Markdown details to sanitized HTML. Leptos renders summaries as escaped text. A missing history has an explicit first-release state; malformed files produce an error instead of silently removing updates.

The `/changelog` components and stylesheet own the visual layout. Change those files to customize the presentation without changing the release-note generator. The public JSON files at `/releases/<version>.json` can also feed another website, an in-app update view, or an update feed.

## Deploy after release approval

The existing release workflow prepares a release PR. A maintainer approves and merges that PR through the repository's required checks. Post-merge automation creates release tags and draft releases, then calls `app-deploy.yml` only when the release record contains `monochange_app`.

The deployment workflow verifies the exact commit, tag, and recorded website artifacts. It builds and smoke-tests a Linux image, transfers it through a pinned SSH connection, backs up SQLite, and restarts the application with its existing runtime credentials. Public HTTPS, health, JSON, and changelog checks must pass before automation publishes the website's GitHub release with the recorded user notes. CLI publication remains separate.

Production deployments are serialized. Failed deployments leave the website release as a draft and report the failure in Actions. Inspect migrations and backups before a rollback, because an older image may not support the new database schema. Re-run the failed deployment job after fixing its cause; do not create another tag or release record.

The workflow needs a dedicated `WEBSITE_DEPLOY_SSH_KEY` secret in the `website-production` Actions environment and the corresponding forced-command key on the server. Restrict that environment to `main`. Keep the key separate from personal SSH access. Production passwords stay in the runtime vault, and the private deployment key belongs in the recovery vault. See `app/deploy/digitalocean/OPERATIONS.md` for the server setup and credential rotation procedure.

<!-- {/websiteReleaseNotes} -->
