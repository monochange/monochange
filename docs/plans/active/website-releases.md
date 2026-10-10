# Website releases

## Outcome

Version `monochange_app` independently, deploy every approved website release, and show its user-facing notes at `/changelog`. Keep CLI releases and registry publication separate.

## Design

- Register the private application as a Cargo release target with namespaced tags and no registry publishing.
- Route website changeset types to a dedicated `website` stream. Generate cumulative Markdown and one structured JSON file per version.
- Render committed JSON artifacts with the site's own components. Keep the first-release empty state honest and surface malformed artifacts as failures.
- Deploy from the dedicated `app` workflow triggered by the release merge push, waiting for the batch tag because tags created with `GITHUB_TOKEN` do not start another workflow.
- Build and test the exact release commit on Actions, transfer the image through pinned SSH, back up SQLite, deploy, and verify public HTTPS. Serialize production rollouts.
- Use a separate deployment key stored through Monosecret and the recovery vault. Keep runtime passwords on the server.
- Document this repository's configuration as a reusable book example.

## Checklist

- [x] Prove package discovery, independent version writes, stream routing, and generated notes.
- [x] Add the changelog page and cover normal, empty, malformed, and escaped-content behavior.
- [x] Add release deployment and validation workflows.
- [x] Set up the deployment credential without reusing the personal SSH key.
- [x] Update shared documentation and user-facing changesets.
- [x] Run required checks and review the complete diff.
- [x] Review the changelog in Chrome at desktop and mobile sizes.
- [x] Open a signed PR, pass CI, and merge through the queue.
- [ ] Verify the deployed website. Release PR approval remains a maintainer action.

## Local verification

The app's 91 tests, Clippy, SSR/WASM build, documentation synchronization, workflow security scan, and repository lint pass. All 4,067 repository Rust tests pass with `TERM=xterm-256color`; the version-output snapshots pass with `TMPDIR=/private/tmp`, avoiding macOS's `/var` alias. The native release fixture proves that website notes and versions do not stamp the CLI manifest or schedule registry publication.

The pre-existing agent-free skill contracts still expect older discovery formatting and exit codes; their local run fails without changes to the CLI or those contracts. The app's changed executable lines have 100% patch coverage, including real HTTP requests through the SSR server. Chrome review covered the populated changelog at desktop and mobile sizes, dark mode, and the mobile menu. [The implementation PR](https://github.com/monochange/monochange/pull/751) passed its head checks and entered the normal merge queue. The queue's first lint run hit a GitHub download rate limit while installing cargo-deny; that infrastructure failure requires a retry before merge.

## Release follow-up

[The implementation PR](https://github.com/monochange/monochange/pull/751), [integrated book PR](https://github.com/monochange/monochange/pull/752), and [approved release PR](https://github.com/monochange/monochange/pull/747) merged through the normal queue. The release's post-merge workflow stopped at website selection because the clean devenv shell removed `RELEASE_COMMIT`. Keep this non-secret commit identifier alongside the existing `RELEASE_TAG` allowlist entry so both selection and the deployment identity check receive their inputs. Verify the real shell boundary with synthetic values and the committed release record; do not manually rerun release or publishing operations.

[The environment repair](https://github.com/monochange/monochange/pull/754) merged and automation prepared [the website patch release](https://github.com/monochange/monochange/pull/755). Its initial opening event preceded the automatic `release` label, so the changeset policy received no labels and rejected an already prepared release. Read the PR's current labels immediately before checking policy, with JSON encoding for the step output and no fallback to stale event data. Verify that metadata boundary against the generated PR and keep the website-only release preview free of registry publications.

[The label repair](https://github.com/monochange/monochange/pull/756) and [mobile installation repair](https://github.com/monochange/monochange/pull/758) merged. The refreshed release also included the CLI/package changes from [the release primitives PR](https://github.com/monochange/monochange/pull/757). After approval, the release merged at 12:33 UTC on 7 October 2026. Its automatic release selection, image build, and image verification passed, but production transfer failed because the runner received an empty deployment key. The `website-production` environment contains the key; the reusable-workflow caller omitted secret inheritance. This matches [the GitHub runner environment-secret issue](https://github.com/actions/runner/issues/4453).

Run website deployment directly in the existing post-merge CI workflow, with the same production environment, permissions, release identity checks, and serialized rollout. This removes the environment-secret handoff to a reusable workflow without forwarding unrelated secrets. Validate the private key before setup or image building, and always remove its temporary file, including on earlier failures. Verify the actual workflow step with generated test keys and empty, malformed, and encrypted input without reading production credentials. Ship the repair through an ordinary signed PR and a fresh approved release. Do not rerun the failed release job or alter its tags, releases, or publication state. Production remains on the previous image until a new release deploys successfully.

The stranded [v0.17.0 release](https://github.com/monochange/monochange/releases/tag/v0.17.0) showed the same env-scrub failure one step earlier in the job: website selection aborted before `dispatch release publishing`, so no publish workflow ever ran for 0.17.0 and its packages never reached crates.io or npm. With maintainer approval, the accumulated drafts were resolved: schema and snapshot releases whose crate versions exist on the registries were published, while phantom drafts for versions that never reached a registry or production (`v0.3.1`, `v0.17.0`, schema 0.9.1, classification 0.4.1, and app 0.2.0/0.2.1) and every monochange_classification draft were deleted. Going forward the publish dispatch runs first, publish.yaml publishes the batch's sibling drafts after the registry publish, and monochange_classification keeps its tag but skips the provider release. The website deployment moved out of ci.yml into its own `app` workflow that selects the release commit, waits for the app tag, and deploys through the production environment; ci.yml left `monochange_app`'s affected paths in favour of that workflow file, so general CI changes never require a `monochange_app` changeset. The maintainer-only `no-changeset-required` label joined `skip_labels` for any remaining plumbing pull requests. A new `lint:devenv-env` check fails workflow steps whose env variables the cleaned devenv shell would remove; it also caught the silently disabled benchmark budget guard, whose `BENCHMARK_VIOLATIONS` input is now allowed through `clean.keep`.
