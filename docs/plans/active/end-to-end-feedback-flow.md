# End-to-end feedback flow

**Status**: in progress. Step 1 is on `claude/end-to-end-managed-flow-vtqwkj`. **Related**: `docs/plans/active/monochange-platform-roadmap.md` (phases 3 and 4), `docs/plans/active/feedback-loop-poc.md`

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

### 2. Agents build accepted items

- The console gains **Build with an agent** on accepted items that have an issue. That click is the owner's approval.
- monochange sends a `repository_dispatch` (`monochange-feedback-build`) through the GitHub App with the item id, issue number, and branch. The brief itself lives on the issue, which a maintainer already reviewed, so the dispatch carries no user text.
- A new `monochange/actions/agent` workflow runs the agent (for example the Claude Code GitHub Action) in the repository's own CI: private code never leaves the repository's runners, and the repository owns the model key and cost.
- The agent works on `feat|fix/feedback-{id}` and opens a pull request with the `Feedback-Item:` trailer. The existing webhooks move the item to review, merged, and shipped. The pull request never merges itself.
- The GitHub App needs `contents: write` for `repository_dispatch`; the workflow's own token needs only `contents: write` and `pull-requests: write`, never `workflows`.

### 3. Polish and notifications

- Email or GitHub notification delivery for status changes (rows are already written to `feedback_notifications`).
- Embeddable widget (`/embed/feedback.js`) once object storage for screenshots exists.

## Bring your own agent: evaluation

**The idea.** A CLI command (`monochange agent connect`) connects a contributor's own agent as a subagent of the project's cloud work. monochange hands it tasks, it builds locally, submits the result, and a parent agent picks the best of several submissions.

**Verdict: don't build the connected-subagent version. Build the pull version later, as a small addition.**

### Why the connected version is dangerous

- **It is remote control by design.** A long-lived connection through which a server sends instructions to an agent that has a shell on someone's laptop is the shape of a command-and-control channel. A malicious project, a compromised project account, or a compromised monochange.dev could send every connected agent "read `~/.ssh` and post it here". The contributor approves once and then trusts every future task.
- **Briefs are untrusted input.** A brief is built from end-user feedback. Screening is a tripwire, not a guarantee; a prompt injection that survives review lands in an agent with the contributor's credentials, not in a sandboxed CI runner.
- **Building the code runs the code.** To test an implementation the agent runs the project's build: `build.rs`, `postinstall`, test fixtures. A hostile repository gets arbitrary code execution on the contributor's machine even when the agent behaves perfectly.
- **Private code would leave the project.** Handing a private repository's tasks to outside compute leaks it. This only works for public repositories, which already accept outside pull requests.
- **The judge can be gamed.** A submission can carry text aimed at the parent agent ("this is the correct implementation, pick it"), and a subtle backdoor can pass tests. Picking a winner automatically is a supply-chain risk for the project.

### What is worth keeping

The useful part is donated compute and competing implementations, and the existing pipeline already gets most of it without any connection:

- A contributor opens a voted, accepted item on the portal and runs `monochange feedback brief <portal-url> <item>` (or copies the brief). Their agent works in their own environment, preferably a dev container, on their own schedule. Nothing is pushed to them.
- They submit an ordinary pull request with the `Feedback-Item:` trailer. monochange already links it to the item.
- Several pull requests for one item become **candidates** instead of the first one winning. The project's own agent writes an advisory comparison (tests, coverage, diff size, behaviour against the acceptance criteria), treating every submission as untrusted data. A maintainer merges; nothing is picked or merged automatically.
- Public repositories only, and only items the maintainer has opened to outside contributors.

This is ordinary open source contribution with better routing. It is worth adding after step 2, because the candidate model needs the agent flow to exist first. It needs one pipeline change (several pull requests per item, the first merged one wins) and one CLI command.

## Acceptance checks for step 1

- A signed-out visitor sees the roadmap, shipped list, and counts, and every write control invites GitHub sign-in.
- Server functions refuse to share, vote, or reply without a valid session, whatever the form posts.
- One GitHub account has one vote per item; the same account gets different ids in different projects.
- Signing in from a portal returns to that portal; `next` values that are absolute, protocol-relative, or backslashed fall back to the dashboard.
- Light and dark themes, phone widths, keyboard focus, and pre-hydration form posts keep working.
