# Feature-request gateway

## Product direction

Make monochange the place where a user's request can be followed through to a shipped change. A request board alone is familiar and easy to replace. The useful difference here is the connection between a request, implementation evidence, and the release notes the user eventually reads.

The CLI, book, and open-source release planning stay free. A hosted service could charge teams for private request boards, collaboration, and custom branding. Pricing and billing need a separate decision; this plan adds no paid service or model usage.

## First version

1. A maintainer connects a repository and opens its request board.
2. A user submits a title, the problem they face, and the outcome they want. Other users can follow the request.
3. The maintainer accepts, declines, or asks for clarification. Acceptance names who will take the work.
4. The maintainer downloads or copies a Markdown handoff for their own agent. It contains the request, acceptance criteria, repository link, and relevant project instructions.
5. The maintainer links the resulting issue or PR. The request shows the evidence behind its progress.
6. A published release note references the request. The board shows the shipped version and a link to those notes.

Use the existing GitHub sign-in for the first version. This keeps account setup small, but requires end users to have GitHub accounts. Email sign-in should be a deliberate later addition if the audience includes people who do not use GitHub.

## Status and evidence

| Status              | What the user can trust                                              |
| ------------------- | -------------------------------------------------------------------- |
| Submitted           | Their request was saved                                              |
| Needs clarification | The maintainer needs more information                                |
| Planned             | A maintainer accepted the request                                    |
| In progress         | The assigned maintainer or their agent explicitly started work       |
| In review           | An implementation PR is linked and open                              |
| Released            | Published release notes identify the request and the shipped version |
| Declined            | The maintainer supplied a reason                                     |

A merged PR is not proof of release. Request status must retain that distinction. Record each transition with its actor, time, explanation, and optional issue, PR, or release link.

## Small implementation

Extend the app's existing repository ownership and session checks. Replace the feedback and roadmap server-function stubs with real persistence rather than create a second request system.

- Store requests, followers, and status events in the existing SQLite database.
- Give each request a stable public URL and a short identifier suitable for a changeset.
- Enforce repository ownership for acceptance, assignment, and status changes. Users can edit their own requests and follow public ones.
- Offer a manual handoff first. Later add an authenticated CLI or MCP operation for an owner's agent to claim work and post evidence.
- Reuse monochange's release-note links to connect request identifiers to released changes.
- Keep agent execution outside monochange in this version. Do not ask users to upload a model key or buy credits.

Start with one public board for monochange itself. Prove that a user can follow a real request from submission to release before adding multi-team billing or background agent execution.

## Acceptance checks

- A submission survives a server restart and is visible to its author.
- Another user cannot edit it or move it through maintainer statuses.
- Followers see the current status and its supporting evidence.
- The handoff can be used by an existing agent without an integration account.
- Linking a merged PR leaves the request awaiting release.
- Linking published notes shows the actual version and changelog entry.
- Empty, loading, authorization, and unavailable states are visible and accessible.

Implementation is separate from the current book and website release PRs.
