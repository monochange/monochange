# Distribution targets

**Status:** proposal — not approved, not started. **Audience:** maintainer decision document.

## Problem

`monochange` understands exactly one kind of publication: pushing a version to a package registry. A workspace that also ships an iOS app, an Android app, an over-the-air patch channel, or an on-chain program has no way to express any of the following:

- this release owes an App Store submission at `1.2.3` build `45`
- this release is eligible for an OTA patch but must not go to the store
- this release is staged on-chain but not yet executed
- the previous store submission was rejected, so this is attempt 2 of the same version
- the on-chain program was verified at commit `abc123`, and that verification is now stale

Today the only options are `publish.enabled = false` (silently skip it) or a bare `Command` step (run something, record nothing). Neither lets `monochange` answer "what is being published, and is it ready?", which is the question the tool exists to answer.

## Prior art in this repository — read this first

An earlier `deployments` subsystem was **removed** in commit `49f69b8ab` (`refactor: remove deployments feature (#37)`, April 2026). The commit message is explicit about why:

> Deployments are a CI concern better handled by native CI workflow triggers and environment gates.

The removed shape was:

```rust
pub enum DeploymentTrigger {
	Workflow,
	ReleasePrMerge,
	ReleasePublished,
}

pub struct DeploymentDefinition {
	pub name: String,
	pub trigger: DeploymentTrigger,
	pub workflow: String, // names a CI workflow file
	pub environment: Option<String>,
	pub release_targets: Vec<String>,
	pub requires: Vec<String>,
	pub metadata: BTreeMap<String, String>,
}
```

That design was redundant with CI, and it was deleted for the right reason. It modeled a **trigger** and a **pointer to a workflow**. It carried no target identity, no version binding, no outcome, no state, no idempotency, and no readiness. `on: pull_request: types: [closed]` in a GitHub workflow did the same job.

Any replacement must clear that bar. What makes this proposal non-redundant is that it models the **publication itself** — what artifact, at what version, delivered where, in what state, and whether it is still owed — which is release state, not CI scheduling. Section "Why not just use CI triggers" below states the boundary explicitly.

## Scope

- Declare distribution targets in `monochange.toml` (`[distribution.<id>]`).
- Bind each target to a configured package, a version, an optional binary/build identity, and a changelog stream.
- Compute which targets a planned release owes, from changeset types, via the existing stream routing.
- Persist per-target intent and outcome in the durable release record.
- Report distribution readiness alongside registry publish readiness.
- Model the reviewed-submission loop (submit → review → reject → resubmit) and the on-chain loop (stage → propose → approve → execute) with one shared state machine.

## Non-goals

- **Do not implement vendor API clients.** No App Store Connect client, no Play Developer API client, no Shorebird client, no Solana RPC client. See "Option 2" below.
- Do not re-introduce `DeploymentTrigger`, `[[deployments]]`, or CI workflow references.
- Do not model CI environment gates, approvals in CI, or workflow dispatch.
- Do not change SemVer planning, version allocation, or the existing registry publish path.
- Do not build a CodePush target. App Center and CodePush were retired on 2025-03-31 and `microsoft/code-push-server` was archived 2025-05-20. Legacy-only.

## Why not just use CI triggers

The removed feature failed this test; this one must pass it. Split the work by asking **who owns the fact**:

| Fact                                                           | Owner                  | Why                                                               |
| -------------------------------------------------------------- | ---------------------- | ----------------------------------------------------------------- |
| When to run (on merge, on tag, on schedule)                    | CI                     | CI already has declarative triggers and environment gates.        |
| Which changesets imply a store release vs an OTA patch         | `monochange`           | Only `monochange` sees changeset types and stream routing.        |
| The version and build number to submit                         | `monochange`           | Version allocation is its core job.                               |
| Whether this version was already submitted, and how many times | `monochange`           | Durable release state lives in `.monochange/releases/`.           |
| Whether a rejection applies to the current version             | `monochange`           | Requires joining the attempt to the release record.               |
| The mechanics of talking to Apple/Google/Shorebird/Solana      | **The user's command** | Vendor churn, credentials, and ToS are not `monochange`'s to own. |

Everything in the right column except the last row is release state. That is the non-redundant core.

## Three options considered

### Option 1 — User-land only (document `Command` recipes)

Add nothing; document how to wire `fastlane deliver`, `shorebird patch`, `solana program upgrade` into `[cli.*]` steps using `Command` and `when`.

Cheapest and zero-risk. It genuinely already works for _execution_: `Command` steps can run anything, `id = "..."` exposes `steps.<id>.stdout`, and `when` expressions gate steps. But it cannot compute eligibility, cannot tell a release plan that a store submission is owed, cannot record an attempt, and cannot answer whether a rejection applies. It leaves the interesting half unsolved.

**Verdict:** necessary as a fallback, insufficient as a design.

### Option 2 — Built-in vendor adapters

Add `monochange_appstore`, `monochange_play`, `monochange_solana` crates that call the vendor APIs directly, mirroring `monochange_github`'s role.

Rejected, for four independent reasons:

1. **It contradicts the existing registry design.** `monochange` does not implement the npm, crates.io, pub.dev, PyPI, or JSR APIs. `PublishAdapter::build_release_command` returns a `CommandSpec { program, args, cwd, env, timeout }` and shells out to `npm publish`, `cargo publish`, `dart pub publish`, `deno publish`, `uv publish`. Distribution should follow the pattern that already works.
2. **It violates a hard product rule.** `docs/agents/product-rules.md` requires first-class support on macOS, Linux, and Windows. The practical toolchains are `fastlane` (Ruby), `xcrun` (macOS-only), and `dapp-store` (Node) — none portable. Shelling out to a user-configured command sidesteps this entirely; bundling clients would break it.
3. **Credential and ToS exposure.** App Store Connect needs ES256 JWT API keys; Play needs a service-account JSON with `androidpublisher` scope; Solana needs an upgrade-authority keypair. `AGENTS.md` already forbids the agent from using local registry credentials. Owning vendor credentials inside the tool widens that surface considerably.
4. **Vendor churn is real and fast.** See "Evidence for the thin model" below.

### Option 3 — Declared targets, capability-typed state, user-supplied executors _(recommended)_

`monochange` owns the **intent, identity, eligibility, state, and audit trail**. The user supplies the **command** that does the work. This is the registry pattern (`npm publish` as a `CommandSpec`) generalized to non-registry destinations.

## Evidence for the thin model

The vendor APIs churn faster than a release tool should. All of the following were verified during research:

- Apple's `appStoreVersionSubmissions` resource is **deprecated** — it has no create verb left, only a deprecated `DELETE`. The current mechanism is `POST /v1/reviewSubmissions` → `POST /v1/reviewSubmissionItems` → `PATCH /v1/reviewSubmissions/{id}` with `{submitted: true}`.
- `AppStoreVersionState` is deprecated in favour of `AppVersionState`, and the well-known terminal state **`READY_FOR_SALE` does not exist** in the replacement enum. The modern terminal states are `READY_FOR_DISTRIBUTION`, `PENDING_APPLE_RELEASE`, and `PENDING_DEVELOPER_RELEASE`.
- On `ReviewSubmission`, `submitted` and `canceled` are **write-only command flags** present on the update request and absent from the read schema. A tool that mirrors vendor fields as its own state will get this wrong.
- Google Play's `TrackRelease.publishingState` **does not exist in the current v3 API** (zero occurrences in the live discovery document, revision 20260917). Review state moved to a separate read surface: `applications.tracks.releases.list` returning `ReleaseSummary.releaseLifecycleState`.
- Sourcify's **v1 API was switched off on 2026-07-07**; only `/v2/` remains.
- CodePush retired 2025-03-31.

The conclusion is not "never model vendor state." It is: **model the decision points `monochange` can act on, and let the user's command translate vendor specifics.** Mirroring vendor state spaces means inheriting their deprecations.

## Recommended model

### The unifying insight

Three publication shapes that look unrelated are structurally the same release problem:

- a store submission awaiting review
- a Solana program staged in a buffer and awaiting a Squads threshold
- an OTA patch published to a channel

All three are: _an artifact, bound to a version, delivered somewhere, which is not live yet, and whose promotion to live is decided by something outside `monochange`._ Review approval and multisig approval are the same state with different names. That is why one state machine serves all of them.

### Kinds and capabilities

Follow the existing `SourceCapabilities` precedent (`crates/monochange_core/src/lib.rs:6340`) — declare capabilities as data rather than making every struct field optional.

```rust
pub enum DistributionKind {
	/// Reviewed store submission: App Store, Google Play, Solana dApp Store.
	StoreReview,
	/// Over-the-air payload: Shorebird, EAS Update.
	OverTheAir,
	/// On-chain program or contract publication.
	OnChain,
	/// Anything else the workspace owns entirely.
	External,
}

pub struct DistributionCapabilities {
	/// A third party must approve before users receive the artifact.
	pub external_review: bool,
	/// A rejected attempt can be resubmitted for the same version.
	pub resubmittable: bool,
	/// Delivery can be withdrawn or rolled back after going live.
	pub reversible: bool,
	/// The artifact is staged before it becomes live.
	pub staged: bool,
	/// A separate authority must approve execution (multisig, timelock).
	pub requires_approval: bool,
	/// Delivery binds to a binary/runtime identity, not just a version.
	pub binary_identity: bool,
}
```

Kind determines defaults; per-target overrides are allowed. This mirrors the `PackageType::GitHubActions` preset (`publish_enabled: Some(false)`), which already exists in config.

### State machine

```
Planned ──▶ Staged ──▶ Submitted ──▶ InReview ──▶ Approved ──▶ Live
                │           │            │
                │           │            └──▶ Rejected ──┐
                │           │                            │
                │           └──▶ Withdrawn               │
                └──▶ Failed ◀────────────────────────────┘
```

Reachability by kind, so most targets only use a subset:

| State       | Registry | Store                | OTA         | On-chain             |
| ----------- | -------- | -------------------- | ----------- | -------------------- |
| `Planned`   | ✓        | ✓                    | ✓           | ✓                    |
| `Staged`    | —        | ✓ binary built       | —           | ✓ buffer written     |
| `Submitted` | ✓        | ✓ sent for review    | ✓ published | ✓ proposed           |
| `InReview`  | —        | ✓                    | —           | —                    |
| `Approved`  | —        | ✓ passed review      | —           | ✓ threshold met      |
| `Live`      | ✓        | ✓ released           | ✓ serving   | ✓ deployed           |
| `Rejected`  | —        | ✓                    | —           | —                    |
| `Withdrawn` | —        | ✓ pre-release cancel | ✓           | ✓ proposal cancelled |
| `Failed`    | ✓        | ✓                    | ✓           | ✓                    |

Deliberately **not** included: vendor states such as `PENDING_DEVELOPER_RELEASE`, `METADATA_REJECTED`, `PROCESSING_FOR_DISTRIBUTION`, `RELEASE_LIFECYCLE_STATE_APPROVED_NOT_PUBLISHED`. Those belong to the vendor's translator, not to `monochange`.

### Record shape

Key modelling decision: **the submission attempt is the unit, not the version.** A rejected submission is resubmitted against the same version, so a version can accumulate attempts.

```rust
pub struct DistributionTargetRecord {
	pub id: String,
	pub kind: DistributionKind,
	pub package: String,
	/// Marketing version delivered by this target.
	pub version: String,
	/// Monotonic binary/build identity, when the target requires one.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub build: Option<String>,
	/// Changelog stream that triggered this target, when configured.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub stream: Option<String>,
	/// Named changelog output rendered as the human-facing notes.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub changelog_output: Option<String>,
	pub status: DistributionStatus,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub attempts: Vec<DistributionAttempt>,
}

pub struct DistributionAttempt {
	pub number: u32,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub recorded_at: Option<String>,
	/// Identifier the external system returned.
	///
	/// Apple review submission id, Play release name, Shorebird patch number,
	/// or a chain transaction signature.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub external_id: Option<String>,
	pub status: DistributionStatus,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub detail: Option<String>,
}
```

`external_id` is the deliberate unification point: Apple's submission id, Play's release name, Shorebird's patch number, and Solana's transaction signature are all "the handle the external system gave us."

## Configuration

New top-level table, following the existing `[package.<id>]` / `[group.<id>]` / `[cli.<name>]` id-keyed convention:

```toml
# ── App Store ───────────────────────────────────────────────────────────
[distribution.appstore]
kind = "store_review"
package = "mobile-app" # supplies version + package identity
stream = "user" # only user-stream changes justify this
build = "ios_build_number" # name of the binary identity to stamp
changelog_output = "appstore-notes" # reuse [changelog.outputs.<id>]
submit = "fastlane deliver --submit-for-review"
# optional: command that reports current vendor state, JSON on stdout
status = "fastlane deliver --check-status --json"
required = true # block release readiness until submitted

# ── Google Play ─────────────────────────────────────────────────────────
[distribution.play]
kind = "store_review"
package = "mobile-app"
stream = "user"
build = "android_version_code"
changelog_output = "store-notes"
submit = "fastlane supply --track production --aab app.aab"
required = true

# ── Shorebird OTA patch ─────────────────────────────────────────────────
[distribution.shorebird]
kind = "over_the_air"
package = "mobile-app"
stream = "app_feature" # minor changes eligible for a patch
submit = "shorebird patch android --track stable"
dry_run_command = "shorebird patch android --dry-run"

# ── Solana program ──────────────────────────────────────────────────────
[distribution.solana_program]
kind = "on_chain"
package = "program"
stream = "breaking" # only breaking changes justify a redeploy
submit = "solana program upgrade target/deploy/program.so <PROGRAM_ID>"
required = true
# optional, records who must approve before `Live`
approval = { scheme = "squads_v4", address = "<MULTISIG_PDA>", threshold = 3 }
```

### Why `stream` is the right trigger

This is the load-bearing design decision, and it reuses machinery that already exists and is already validated.

`AGENTS.md` already carries the convention:

> For mobile apps, use a configured `native` major type when native binaries or app-store distribution are required. Use a configured `app_feature` minor type for changes eligible for a Shorebird release. The configured type—not subjective wording—must drive this release decision.

That convention has no mechanism behind it today. Streams supply one:

- `[changelog.types.<type>].stream` already routes each change type to a stream (`crates/monochange_core/src/lib.rs:2369`).
- Config already **rejects a changeset whose targets resolve to more than one stream** (`crates/monochange_config/src/lib.rs:2801`), with the diagnostic "split the changes into one file per stream."
- Named outputs already select a stream and render only its content.

So "was this release native-affecting?" reduces to "did any changeset land in the `native` stream?" — already computed, already unambiguous, and already auditable in the release record (`ReleaseManifestChangelog` carries `output` and `stream`).

The one-stream-per-changeset rule is what makes this sound. Without it, a target's trigger would be ambiguous. It exists.

Two consequences worth stating plainly:

- **Eligibility is derived, not configured per release.** `app_feature` changes route to the OTA target; `native` changes route to both stores. Nobody hand-writes "please submit this one."
- **A target with no matching stream is not owed.** If a release contains only docs changes, the store submission is not owed and readiness stays green.

## Readiness

Distribution readiness answers, per target: _is this target owed by this release, and if so, is it satisfied?_

Inputs it needs, all already available:

- the release record from git history (`discover_release_record`, `crates/monochange/src/release_record.rs:56`)
- which streams the release's changesets resolved to (from `ReleaseManifestChangelog`)
- configured `[distribution.*]` targets

Output reuses the existing readiness conventions: a global status plus per-target status, a stable `kind` discriminator, and a `schema_version`. `PublishReadinessReport` (`crates/monochange/src/publish_readiness.rs:80`) is the template.

Recommended statuses per target: `not_owed`, `planned`, `in_progress`, `satisfied`, `blocked`, `rejected`.

**Open question (recommendation: separate surface).** Either extend `PublishReadinessReport` with a `distributions` array, or add a sibling report. I recommend a sibling (`distribution-readiness`), because `publish-readiness` builds an `input_fingerprint` over registry-specific inputs (manifests, lockfiles, `.npmrc`, `pyproject.toml`) and validates registry-specific preconditions — the fingerprint concept does not transfer to "did Apple approve this," and overloading the schema would make both harder to reason about. Revisit if command-surface cost outweighs it.

## Build number / binary identity

**This is a prerequisite, not a detail.** `monochange` has no concept of a build number today — no `versionCode`, no `CFBundleVersion`, no `1.2.3+4`. Verified: zero occurrences of `versionCode`, `version_code`, or `build_number` across all Rust crates.

All three mobile targets need one, and they constrain it differently — and the difference is the key design input:

- **Apple, iOS** — build numbers are unique **per release train** (per version string), not globally. Archived TN2420: "For iOS apps, build numbers must be unique within each release train, but they do not need to be unique across different release trains." So `1.0.0` (build 1) followed by `2.0.0` (build 1) is allowed; the reject→resubmit loop increments the build _within_ the train.
- **Apple, macOS** — the opposite: build numbers must monotonically increase even across versions.
- **Google Play** — `versionCode` is global per app, monotonic across all tracks, permanently consumed, max 2,100,000,000.
- **Shorebird** — release identity _is_ `version+build` (`1.0.0+1`), and a patch binds to that exact pair.

Because one app genuinely needs two counters with different scopes (a train-scoped iOS build number and a never-resetting Android `versionCode`), build numbers must be a named, multi-axis concept rather than a single field.

**Superseded by a full design.** [`version-schemes-and-build-numbers.md`](./version-schemes-and-build-numbers.md) now owns this (direction approved and revised 2026-09-19): per-package declared values with source/behaviour/reset (`increment`, `reset = "version"` for iOS trains vs `reset = "never"` for Play), counters stored in user-created files with dotted-field extraction, and `value_template` on versioned files. One identity version per package lives only in the release record. The resubmission loop composes later: an App Store attempt is an explicit amend-and-recommit against the existing record that increments the train-reset counter.

## OTA: does the API need to know about it?

This was an explicit question. The answer is split:

**Execution: no.** `Command` steps already run `shorebird patch` or `eas update` today. Nothing is needed.

**Eligibility: yes.** The one fact that matters for OTA — _is this change compatible with the already-shipped binary?_ — is the thing `monochange` uniquely knows.

The research is unambiguous on the mechanism:

- **Shorebird**: a patch binds to a specific `release_id` and version (`1.0.0+1`), carries an auto-incrementing patch number, and only one patch is active per track.
- **EAS Update**: `expo-updates` loads a remote update only if the binary's `runtimeVersion` matches exactly. Under the `appVersion` policy, forgetting to bump the app version after a native change ships an update that fails on device or is silently auto-rolled-back.
- **CodePush**: retired; excluded.

So the correct boundary is: `monochange` decides **whether a change is OTA-eligible** (from streams) and **which binary it binds to** (from the build number), then hands the transport to a user command. It should not know about channels, branches, or rollout percentages.

A valuable validation rule falls out: **a release that changes native-affecting code without advancing the binary identity should be an error, not a warning.** Under EAS's `appVersion` policy this is a silent production failure, and `monochange` is the only component positioned to catch it before it ships.

## On-chain specifics

On-chain needs two states the store path does not, and one property that is easy to get wrong.

**Staging is real and resumable.** A Solana upgrade is a multi-transaction operation: create and initialize a buffer, write the ELF in chunks, then `Upgrade`. A buffer can exist with its authority already transferred to a multisig vault, not yet executed. This is `Staged` → `Submitted` in the state machine, and it means a release record must represent "prepared but not live" and be resumable.

**Approval is a first-class state.** Squads multisig is propose → approve → execute, with states `Draft`, `Active`, `Approved`, `Executed`, `Rejected`, `Cancelled` and an optional timelock that is **global to the multisig**, not per-proposal. The release is not done when proposed. Note that `transactionIndex` is a monotonic counter — concurrent release attempts race on it.

**Verification expires.** This is the property to model carefully. `solana-verify` stores a PDA (owned by `verifycLy8mB96wd9wqq3WDXQwM4oU6r42Th37Db9fC`, seeds `[b"otter_verify", signer, program_id]`) holding the repository URL, commit hash, and build arguments, writable only by the upgrade authority. **Any program upgrade invalidates it**, and the remote worker re-verifies on its own schedule. Verification is therefore a property with a lifetime, not a boolean flag. A "verified" badge copied into a release record becomes a lie after the next upgrade.

**Rollback is not a primitive.** There is no on-chain archive. The buffer is drained after upgrade and only `ProgramData.slot` changes. Rolling back means redeploying an artifact the release tool must have retained itself. Two permanent one-way doors deserve explicit recording:

- setting the upgrade authority to `None` (`--final`), after which no future release is possible
- `Close` on a ProgramData account, after which the program id can never be used again

Also worth recording, because it is a common failure: Solana programs have no on-chain version field at all. The version is `monochange`'s label; the only monotonic on-chain field is `ProgramData.slot`.

**Extension to EVM/L2s.** The same model holds, with two adjustments: a proxy release is **multiple addresses and artifacts** (proxy + implementation, plus a beacon or N diamond facets), so a target must not assume one address; and verification is an off-chain explorer flag (Etherscan/Sourcify) rather than an on-chain PDA. Safe multisigs propose through an off-chain transaction service and execute on-chain directly — the same propose/approve/execute shape as Squads.

Recommended: ship Solana first, keep `DistributionKind::OnChain` chain-agnostic, and add chains as configured commands rather than new code.

## Affected areas

| Crate                        | Change                                                                                                                                                                                                               |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `monochange_core`            | `DistributionDefinition`, `DistributionKind`, `DistributionCapabilities`, `DistributionStatus`, `DistributionAttempt`, `DistributionTargetRecord`; new `CliStepDefinition` variants                                  |
| `monochange_config`          | parse `[distribution.<id>]`; validate kind/capability consistency, package exists, stream exists, `changelog_output` exists, `build` field resolves, command non-empty; reject `store_review` without a notes output |
| `crates/monochange`          | new `src/distribution.rs` orchestration module (keeping dispatch concentrated, per `docs/agents/architecture.md`); readiness module; release-record plumbing                                                         |
| `crates/monochange_schema`   | release-record schema `0.7` → `0.8` migration adding `distributions`                                                                                                                                                 |
| fixtures / integration tests | `fixtures/tests/distribution-*/`, file fixtures, Insta snapshots                                                                                                                                                     |
| docs + init template         | `crates/monochange/src/monochange.toml.template`, `docs/src/guide/`, CLI step reference, monochange skill                                                                                                            |

Deliberately **no new adapter crate.** There is no provider-specific payload shaping to own, because `monochange` does not shape vendor payloads. If built-in Solana support is ever added, it belongs in a `monochange_solana` crate implementing the same capability contract, mirroring `monochange_github`.

## Implementation cost note

Adding a `CliStepDefinition` variant is not a one-line change. The enum is `#[serde(tag = "type", deny_unknown_fields)]` and `#[non_exhaustive]`, and each variant must be threaded through roughly ten match arms in `crates/monochange_core/src/lib.rs` (`inputs`, `with_inherited_step_inputs`, `name`, `when`, `always_run`, `show_progress`, kebab-case naming, `step_inputs_schema`, and the `None` returns around lines 4043 and 4108), plus a documentation page under `docs/src/reference/cli-steps/`, the CLI snapshot baseline at `.monochange/cli-snapshots/monochange.json`, and the `docs:check` and `lint:architecture` gates. Worth knowing before promising a small change surface.

## Phasing

**Phase 0 — binary identity (prerequisite).** Build numbers as named per-package axes with train/owner scope, per [`version-schemes-and-build-numbers.md`](./version-schemes-and-build-numbers.md) (implementation plan: PRs 1–3). Blocks everything mobile.

**Phase 1 — declaration and validation.** `[distribution.<id>]` parsing, domain types, capabilities, config validation, init template, JSON schema. No behaviour change yet.

**Phase 2 — planning and readiness.** Compute owed targets from release streams; carry `DistributionTargetRecord` in the manifest and release record; ship `distribution-readiness`. This is the first phase that answers "what is being published?"

**Phase 3 — the review loop.** Record submission attempts, model rejection and resubmission, allow multiple attempts per version, surface store note length limits.

**Phase 4 — on-chain.** `Staged` and approval states, staging resumability, verification-with-lifetime, recording of authority state and permanent one-way doors.

**Phase 5 — optional reconciliation.** Let `status_command` return JSON on stdout so readiness can report live vendor state without `monochange` owning a client. Requires a documented, versioned JSON contract; defer until Phases 1–3 prove the model.

Suggested first cut for a single PR: **Phase 1 only.** It is additive, testable in isolation, and reversible.

## Validation

```bash
devenv shell fix:all
devenv shell build:all
devenv shell lint:all
devenv shell test:all
devenv shell docs:update
devenv shell docs:check
devenv shell coverage:patch
devenv shell monochange step validate
```

Per `AGENTS.md`: integration tests belong in `crates/monochange_integration_tests` using file fixtures and Insta snapshots; unit tests live in `__tests__/<module>_tests.rs`; patch coverage must reach 100% for executable changed lines; run `snapshot:update` and delete unreferenced `.snap` files.

## Decisions needed

| # | Decision                                                                               | Recommendation                                                                                                        |
| - | -------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| 1 | Accept Option 3 over Option 1 (user-land) and Option 2 (vendor adapters)?              | Yes — Option 3.                                                                                                       |
| 2 | Top-level name: `[distribution.<id>]`, `[delivery.<id>]`, or `[publish.targets.<id>]`? | `distribution`. It is unclaimed (verified), accurate, and avoids overloading `publish`, which today means "registry." |
| 3 | Separate `distribution-readiness`, or extend `publish-readiness`?                      | Separate, because registry input fingerprints do not transfer.                                                        |
| 4 | Phase 0 (build numbers) in scope, or defer?                                            | In scope and first. It blocks every mobile target and is needed to make OTA eligibility sound.                        |
| 5 | Solana in the first implementation, or stores first?                                   | Stores first (Phases 1–3), then on-chain. Store submission is the simpler loop and validates the state machine.       |
| 6 | Does `monochange` ever own a vendor API client?                                        | No. If built-in support is later wanted, add an adapter crate behind the same capability contract.                    |

## Open risks

- **Stream-as-trigger assumes stream discipline.** If a team puts store-relevant changes in the `default` stream, a store target bound to `user` will not fire. Mitigation: validate at config time that each `store_review` target's stream is declared, and warn when a release ships a stream with no distribution target bound to it.
- **Store notes have hard, undocumented-until-runtime limits** (Apple caps What's New, Play caps per-language length). Existing `ChangelogFormat` renderers enforce no length limit. A target may need a `max_length` and a truncation policy.
- **Non-automatable steps must be surfaced, not hidden.** Two verified examples: sending Play changes held with `changesNotSentForReview` for review is **Console-UI only**, and Apple's Resolution Center — the source of rejection text — is **not in the official API** (fastlane notes it requires Apple-ID auth, not API keys). Rejection reasons must therefore be human-supplied `detail` strings. Design for that rather than pretending the loop is fully automatable.
- **Rejecting `store_review` targets without a configured stream** could be too strict for teams that submit on a schedule rather than per-release. Consider making `stream` optional with an explicit `always = true` alternative.
- **Attempt history could grow unbounded** in `release.json`, which is committed to git history. Consider capping retained attempts.
