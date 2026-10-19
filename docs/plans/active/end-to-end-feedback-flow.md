# End-to-end feedback flow

**Status**: in progress. Steps 1 and 2 are on `claude/end-to-end-managed-flow-vtqwkj`; step 4 is designed and experimental. **Related**: `docs/plans/active/monochange-platform-roadmap.md` (phases 3 and 4), `docs/plans/active/feedback-loop-poc.md`

## Goal

One loop a maintainer can run without leaving monochange.dev:

1. A user signs in with GitHub on a project's portal, shares a bug or an idea, and votes on what others shared.
2. The maintainer accepts what the community wants, and monochange opens the issue in the right repository.
3. An agent builds the accepted item in the repository's own CI and opens a pull request that a maintainer reviews and merges.
4. The release that ships it closes the loop: every voter and the submitter see "Shipped in vX.Y.Z" with release notes.

GitHub is the only repository provider and the only login provider for this work. Accounts are already keyed by `(provider, external_id)`, so more providers are adapters later, not a migration.

## Where things stand

Steps 2 and 4 already work on `main`: projects (#785), the portal and maintainer console (#786), and pull request and release webhooks that advance items (#788). What is missing is a trustworthy voter, a portal that feels finished, and the agent.

## Steps

### 1. GitHub sign-in on the portal (this branch)

Portal visitors were pseudonymous: a random cookie became a per-project id. That is fine for reading, but votes are what decide what agents build, and a cookie id can be minted by clearing cookies. Writing now needs a GitHub account.

- Reading the portal stays public and needs no account.
- Sharing, voting, and discussing require a signed-in GitHub account. The server functions refuse a visitor without a session.
- The visitor's per-project id is derived from their GitHub user id with the server secret and the project id (`account_viewer_id`), so ids still can't be forged, linked across projects, or traced back to a GitHub login from the stored feedback. Maintainers keep seeing pseudonymous ids, as before.
- Sign-in returns to where it started. `/login?next=/p/{org}/{project}` signs the destination into the OAuth state cookie; only same-site paths are accepted, so the callback can't become an open redirect.
- The cookie-only viewer token is retired. Items shared under it keep their votes and stay on the roadmap; their author just no longer sees them under "Yours".

Design: the portal is the first thing a project's users see, so it gets a full pass. Hero with live counts and the visitor's GitHub identity, a composer that invites sign-in instead of failing, tactile vote buttons, a roadmap board grouped by status, a progress tracker on the visitor's own items, and a shipped timeline. Every action is still a plain form that works before hydration.

### 2. Agents build accepted items (this branch)

- The console gains **Build with an agent** on accepted items that have an issue. That click is the owner's approval.
- monochange checks that the issue's repository has `.github/workflows/monochange-feedback-build.yml`, then sends a `repository_dispatch` (`monochange-feedback-build`) through the GitHub App with the item id, repository, issue number, and branch (`BuildRequest`). The brief itself lives on the issue, which a maintainer already reviewed, so the dispatch carries no user text. A missing workflow is an error, so an item never sits "in development" with nothing running.
- The workflow, documented in `docs/src/guide/feedback-agent-builds.md`, runs the agent (the Claude Code action in the example) in the repository's own CI: private code never leaves the repository's runners, and the repository owns the model key and cost. The agent step holds no push credentials; a later step pushes and opens a draft pull request.
- The agent works on `feat|fix/feedback-{id}` and the pull request carries the `Feedback-Item:` trailer. The existing webhooks move the item to review, merged, and shipped. The pull request never merges itself.
- The GitHub App already needs `contents: write` for hosted releases, which covers `repository_dispatch`; the workflow's own token needs only `contents: write`, `issues: read`, and `pull-requests: write`, never `workflows`.
- Later: a reusable `monochange/actions/agent` action can replace the copied workflow.

### 3. Polish and notifications

- Email or GitHub notification delivery for status changes (rows are already written to `feedback_notifications`).
- Embeddable widget (`/embed/feedback.js`) once object storage for screenshots exists.

### 4. Shared compute (experimental)

A project that has more accepted work than compute can share tasks, and contributors run them with their own agents. monochange sits between the two and screens both ends: a task before any contributor can run it, and the work before the project's agent or a maintainer reads it.

The safety rule for this flow: **an agent checking that a task is safe is a tripwire, not a guarantee.** A model reliably catches "print your environment variables"; it does not reliably catch a careful prompt injection or a backdoor that passes tests. So the guarantees come from what the contributor's agent can reach (nothing worth stealing) and from rules a program checks. Model screening sits on top to catch abuse early and keep the queue clean.

**Roles.** The project owner shares tasks and merges work. monochange.dev screens, lists, and routes. A contributor runs `monochange tasks` with their own agent and model key.

**Flow.**

1. **Share.** The owner marks an accepted item as open to contributors. The project's agent can split a large item into several tasks first. A task has a fixed shape: the repository, a pinned commit, the issue it implements, acceptance criteria, the paths it may change, and the check command that proves it (for example `cargo test -p invoices`). Text fields are short and always quoted to the agent as data. The project must be public and opted into the experiment.
2. **Screen the task.** Before a task is listed, monochange checks it:
   - Rules reject anything that asks for environment variables, credentials, tokens, keys, home-directory files, network calls outside the repository's host and package registries, or commands other than the declared check command, and anything that hides content (encoded blobs, invisible characters).
   - A screening model reads the whole task and flags anything that isn't software work on that repository.
   - A rejected task goes back to the owner and is never listed. Every listed task is public, so anyone can audit what was asked.
3. **Pull and review.** `monochange tasks list` shows open tasks, and `monochange tasks show <id>` prints exactly what the agent will receive, byte for byte. `monochange tasks run <id>` asks the contributor's agent to review the task with a fixed safety prompt first. It starts only after the contributor confirms.
4. **Run in a sandbox.** This is what makes "give me your environment variables" harmless:
   - The CLI runs the agent in a container (Docker or Podman, required) with a fresh clone at the pinned commit.
   - The container gets no host environment variables, no home directory, no SSH agent, no git credentials, and no container socket.
   - Network goes only to the model API and the package registries. The contributor's model key stays on the host: the CLI proxies model calls, so the key never enters the container.
   - Time and CPU are capped. The project's build scripts (`build.rs`, `postinstall`) run inside a container that is thrown away.
5. **Submit.** The CLI takes the diff from the container, not a push, and uploads it with a short summary of the agent's work. The contributor never needs a GitHub token for the task.
6. **Screen the work.** Before the project's agent or a maintainer sees it, monochange checks the diff:
   - Rules reject changes outside the task's paths, changes to `.github/workflows`, new dependencies or lockfile changes the task didn't allow, binaries, oversized diffs, and anything secret scanning flags.
   - A reviewing model reads the diff and summary as untrusted data and flags text aimed at reviewers ("pick this one") and code unrelated to the task.
   - Passing work becomes a pull request from monochange's fork, credited to the contributor with a `Co-authored-by:` trailer and carrying the `Feedback-Item:` trailer. The project's CI runs it like any fork pull request, without secrets.
7. **Verify and merge.** Several submissions for one task become candidates. The project's agent writes an advisory comparison (checks, coverage, diff size, fit with the acceptance criteria) and treats every submission as data. The owner merges one; the existing webhooks move the item to review, merged, and shipped, and the other candidates close with thanks. Nothing merges automatically.

**Abuse controls.** The experiment is opt-in per project and public repositories only. Tasks expire. Tasks and submissions are rate-limited per project and per contributor. Anyone can report a task, and a reported project's tasks are paused until a monochange admin looks. Contributors see a project's history (tasks shared, work merged) before running anything.

**Why not a live connection.** A long-lived connection where monochange pushes work to a contributor's agent is shaped like a command-and-control channel: one compromised project or a compromised monochange.dev could reach every connected machine at once. Pulling one screened, public, pinned task at a time, into a sandbox, keeps each run small, visible, and contained.

**Build order.**

- **A. Platform tasks.** Tasks on accepted items, the task rules and screening model, owner review of rejections, and a public task API.
- **B. CLI.** `monochange tasks list|show|run|submit`, the container sandbox, and the model proxy. Claude Code first, then other agent CLIs.
- **C. Platform submissions.** Submission rules and review, pull requests from the monochange fork, candidates, and the comparison.

## Acceptance checks for step 1

- A signed-out visitor sees the roadmap, shipped list, and counts, and every write control invites GitHub sign-in.
- Server functions refuse to share, vote, or reply without a valid session, whatever the form posts.
- One GitHub account has one vote per item; the same account gets different ids in different projects.
- Signing in from a portal returns to that portal; `next` values that are absolute, protocol-relative, or backslashed fall back to the dashboard.
- Light and dark themes, phone widths, keyboard focus, and pre-hydration form posts keep working.
