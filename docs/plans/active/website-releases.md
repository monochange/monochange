# Website releases

## Outcome

Version `monochange_app` independently, deploy every approved website release, and show its user-facing notes at `/changelog`. Keep CLI releases and registry publication separate.

## Design

- Register the private application as a Cargo release target with namespaced tags and no registry publishing.
- Route website changeset types to a dedicated `website` stream. Generate cumulative Markdown and one structured JSON file per version.
- Render committed JSON artifacts with the site's own components. Keep the first-release empty state honest and surface malformed artifacts as failures.
- Call deployment from the existing post-merge release workflow, because tags created with `GITHUB_TOKEN` do not start another workflow.
- Build and test the exact release commit on Actions, transfer the image through pinned SSH, back up SQLite, deploy, and verify public HTTPS. Serialize production rollouts.
- Use a separate deployment key stored through Monosecret and the recovery vault. Keep runtime passwords on the server.
- Document this repository's configuration as a reusable book example.

## Checklist

- [x] Prove package discovery, independent version writes, stream routing, and generated notes.
- [x] Add the changelog page and cover normal, empty, malformed, and escaped-content behavior.
- [x] Add release deployment and validation workflows.
- [x] Set up the deployment credential without reusing the personal SSH key.
- [x] Update shared documentation and user-facing changesets.
- [ ] Run required checks and review the complete diff.
- [ ] Review the changelog in Chrome at desktop and mobile sizes (Mac unlock pending).
- [ ] Open a signed PR, pass CI, and merge through the queue.
- [ ] Verify the deployed website. Release PR approval remains a maintainer action.

## Local verification

The app's 91 tests, Clippy, SSR/WASM build, documentation synchronization, workflow security scan, and repository lint pass. All 4,067 repository Rust tests pass with `TERM=xterm-256color`; the version-output snapshots pass with `TMPDIR=/private/tmp`, avoiding macOS's `/var` alias. The native release fixture proves that website notes and versions do not stamp the CLI manifest or schedule registry publication.

The pre-existing agent-free skill contracts still expect older discovery formatting and exit codes; their local run fails without changes to the CLI or those contracts. The independent app coverage run and browser review are still in progress.
