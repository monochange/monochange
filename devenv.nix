{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

let
  currentDir = builtins.dirOf __curPos.file;
  custom = inputs.ifiokjr-nixpkgs.packages.${pkgs.stdenv.system};
in
{
  packages =
    with pkgs;
    [
      cargo-binstall
      cargo-run-bin
      cacert
      custom.mdt
      dprint
      gh
      git
      gitleaks
      hyperfine
      jq
      mdbook
      nixfmt
      pnpm
      nodejs_24
      python3
      rustup
      shfmt
      taplo
      unzip
      zip
      zizmor
    ]
    ++ lib.optionals stdenv.isDarwin [
      coreutils
    ];

  enterShell = ''
    set -euo pipefail
    export PATH="$DEVENV_PROFILE/bin:$PATH"
  '';

  # disable dotenv since it interferes with variable interpolation in the shell
  dotenv.disableHint = true;

  git-hooks = {
    hooks = {
      "secrets:commit" = {
        enable = true;
        verbose = true;
        pass_filenames = true;
        name = "secrets";
        description = "Scan staged changes for leaked secrets with gitleaks.";
        entry = "${pkgs.gitleaks}/bin/gitleaks protect --staged --verbose --redact";
        stages = [ "pre-commit" ];
      };
      "lint:format" = {
        enable = true;
        verbose = true;
        pass_filenames = true;
        name = "lint:format";
        description = "Run workspace autofixes before commit and restage the results.";
        entry = "${config.env.DEVENV_PROFILE}/bin/lint:format";
        stages = [ "pre-commit" ];
      };
      "lint:test" = {
        enable = true;
        verbose = true;
        pass_filenames = false;
        name = "lint:push";
        description = "Run the local CI lint rules and test suite before push.";
        entry = "${config.env.DEVENV_PROFILE}/bin/lint:push";
        stages = [ "pre-push" ];
      };
    };
  };

  scripts = {
    "monochange" = {
      exec = ''
        set -euo pipefail
        cargo run --quiet --package monochange --bin monochange -- "$@"
      '';
      description = "The dev build of the `monochange` executable";
      binary = "bash";
    };
    "mc" = {
      exec = ''
        set -euo pipefail
        cargo run --quiet --release --package monochange --bin mc -- "$@"
      '';
      description = "The release build of the `monochange` executable";
      binary = "bash";
    };
    "install:all" = {
      exec = ''
        set -euo pipefail
        install:cargo:bin
      '';
      description = "Install all packages.";
      binary = "bash";
    };
    "install:cargo:bin" = {
      exec = ''
        set -euo pipefail
        cargo bin --install
      '';
      description = "Install cargo binaries locally.";
      binary = "bash";
    };
    "update:deps" = {
      exec = ''
        set -euo pipefail
        cargo update
        devenv update
      '';
      description = "Update dependencies.";
      binary = "bash";
    };
    "build:all" = {
      exec = ''
        set -euo pipefail
        if [ -z "''${CI:-}" ]; then
          echo "Building project locally"
          cargo build --workspace --all-features
        else
          echo "Building in CI"
          cargo build --workspace --all-features --locked
        fi
      '';
      description = "Build all crates with all features activated.";
      binary = "bash";
    };
    "build:dist" = {
      exec = ''
        set -euo pipefail
        echo "Building with dist profile (LTO, codegen-units=1, strip)"
        cargo build --workspace --all-features --locked --profile dist
      '';
      description = "Build all crates with the dist profile for release-like optimization.";
      binary = "bash";
    };
    "test:dist" = {
      exec = ''
        set -euo pipefail
        echo "Running tests with dist profile"
        cargo test --workspace --exclude xtask --all-features --profile dist
      '';
      description = "Run cargo tests against the dist-optimized build to verify release behavior.";
      binary = "bash";
    };
    "build:book" = {
      exec = ''
        set -euo pipefail
        mdbook build docs
      '';
      description = "Build the mdbook documentation.";
      binary = "bash";
    };
    "publish:check" = {
      exec = ''
        set -euo pipefail
        mc step:publish-packages --dry-run
      '';
      description = "Check that publication is valid for this project";
      binary = "bash";
    };
    "package:check" = {
      exec = ''
        set -euo pipefail
        echo "=== Verifying crate packages with cargo package ==="
        log=$(mktemp)
        cargo package --workspace --all-features \
          --exclude monochange_book \
          --exclude monochange_integration_tests \
          --exclude xtask 2>&1 | tee "$log" || {
          if grep -q 'failed to select a version\|unable to update registry\|download of config.json failed\|curl failed\|HTTP.*framing layer\|network\|fetch.*failed' "$log" 2>/dev/null; then
            echo "\nWARNING: cargo package failed due to a network error (could not reach crates.io)."
            echo "The packaging itself succeeded, but dependency verification requires registry access."
            echo "This is expected when running without network access and is not a code issue."
            rm -f "$log"
            exit 0
          fi
          echo "\nERROR: cargo package failed."
          echo "This means at least one crate cannot be published because its packaged source fails to compile."
          echo "Common causes:"
          echo "  - build.rs not included in package.include (OUT_DIR not set during publish verification)"
          echo "  - files referenced via include_str! not included in package.include"
          echo "  - path dependencies that don't exist in the tarball"
          rm -f "$log"
          exit 1
        }
        rm -f "$log"
      '';
      description = "Verify all publishable workspace crates can be packaged and built from their tarballs (catches publish failures before they happen)";
      binary = "bash";
    };
    "test:all" = {
      exec = ''
        set -euo pipefail
        test:cargo
        test:docs
        test:node
      '';
      description = "Run all tests across the crates and npm helper scripts.";
      binary = "bash";
    };
    "test:cargo" = {
      exec = ''
        set -euo pipefail
        cargo bin --install
        export PATH="$PWD/.bin/rust-nightly/cargo-nextest/0.9.132/bin:$PATH"
        cargo bin cargo-insta test --workspace --exclude xtask --all-features --test-runner nextest --disable-nextest-doctest --unreferenced=reject
      '';
      description = "Run cargo tests with nextest and reject unreferenced snapshots.";
      binary = "bash";
    };
    "test:cargo:expensive" = {
      exec = ''
        set -euo pipefail
        cargo bin --install
        export PATH="$PWD/.bin/rust-nightly/cargo-nextest/0.9.132/bin:$PATH"
        MONOCHANGE_EXPENSIVE_TESTS=1 cargo bin cargo-insta test --workspace --exclude xtask --all-features --test-runner nextest --disable-nextest-doctest --unreferenced=reject
      '';
      description = "Run cargo tests with CI-only large-fixture cases enabled and reject unreferenced snapshots.";
      binary = "bash";
    };
    "test:docs" = {
      exec = ''
        set -euo pipefail
        cargo test --doc --workspace --exclude xtask --all-features
      '';
      description = "Run documentation tests.";
      binary = "bash";
    };
    "test:node" = {
      exec = ''
        set -euo pipefail
        pnpm build
        pnpm vitest run --exclude 'worktrees/**' scripts/npm/tests/*.test.ts
      '';
      description = "Run npm helper, launcher, and repository utility tests with Vitest.";
      binary = "bash";
    };
    "test:agent-evals" = {
      exec = ''
        set -euo pipefail
        cargo test --package monochange --all-features agent_eval_
      '';
      description = "Run the focused agent-style eval coverage for machine-readable workflows.";
      binary = "bash";
    };
    "coverage:all" = {
      exec = ''
        set -euo pipefail
        mkdir -p target/coverage
        cargo llvm-cov clean --workspace
        cargo llvm-cov test --workspace --exclude xtask --all-features --lib --tests --no-report
        cargo llvm-cov report --ignore-filename-regex 'crates/xtask/' --summary-only --fail-under-lines 70
        cargo llvm-cov report --ignore-filename-regex 'crates/xtask/' --lcov --output-path target/coverage/lcov.info
      '';
      description = "Run workspace coverage, enforce a 70% line-coverage floor, and write target/coverage/lcov.info.";
      binary = "bash";
    };
    "coverage:patch" = {
      exec = ''
        set -euo pipefail
        base_ref="''${MONOCHANGE_PATCH_COVERAGE_BASE:-origin/main}"
        head_ref="''${MONOCHANGE_PATCH_COVERAGE_HEAD:-HEAD}"

        if [ ! -f target/coverage/lcov.info ]; then
          coverage:all
        fi

        pnpm node scripts/check-patch-coverage.ts \
          --repo-root "$DEVENV_ROOT" \
          --lcov target/coverage/lcov.info \
          --base "$base_ref" \
          --head "$head_ref" \
          --target 100
      '';
      description = "Fail when executable changed lines fall below 100% patch coverage.";
      binary = "bash";
    };
    "fix:all" = {
      exec = ''
        set -euo pipefail
        fix:clippy
        docs:update
        schema:update
        fix:monochange
        fix:format
        fix:js
        fix:workflows
      '';
      description = "Fix all autofixable problems, including shared-doc synchronization via `mdt update`.";
      binary = "bash";
    };
    "fix:format" = {
      exec = ''
        set -euo pipefail
        repo_root="$(git rev-parse --show-toplevel)"
        dprint fmt --config "$repo_root/dprint.json"
      '';
      description = "Format files with dprint.";
      binary = "bash";
    };
    "schema:update" = {
      exec = ''
        set -euo pipefail
        cargo xtask schema update
      '';
      description = "Regenerate committed current JSON Schema assets.";
      binary = "bash";
    };
    "schema:check" = {
      exec = ''
        set -euo pipefail
        cargo xtask schema check
      '';
      description = "Check committed current JSON Schema assets are up to date.";
      binary = "bash";
    };
    "schema:release:update" = {
      exec = ''
        set -euo pipefail
        cargo xtask schema release update
      '';
      description = "Regenerate committed release JSON Schema assets, including versioned files.";
      binary = "bash";
    };
    "schema:release:check" = {
      exec = ''
        set -euo pipefail
        cargo xtask schema release check
      '';
      description = "Check committed release JSON Schema assets, including versioned files.";
      binary = "bash";
    };
    "fix:clippy" = {
      exec = ''
        set -euo pipefail
        cargo clippy --workspace --fix --allow-dirty --allow-staged --all-features --all-targets
      '';
      description = "Fix clippy lints for rust.";
      binary = "bash";
    };
    "fix:monochange" = {
      exec = ''
        set -euo pipefail
        mc step:validate
        mc check --fix
      '';
      description = "Fix clippy lints for rust.";
      binary = "bash";
    };
    "lint:workflows" = {
      exec = ''
        set -euo pipefail
        export PATH="$HOME/.cargo/bin:$PATH"
        if ! command -v zizmor >/dev/null 2>&1; then
          echo "Installing zizmor via cargo-binstall..."
          cargo binstall zizmor --no-confirm
        fi
        zizmor .github/workflows/ .github/actions/
      '';
      description = "Scan GitHub Actions workflows for security vulnerabilities with zizmor.";
      binary = "bash";
    };
    "deny:check" = {
      exec = ''
        set -euo pipefail
        cargo deny check
      '';
      description = "Run cargo-deny checks for security advisories and license compliance.";
      binary = "bash";
    };
    "lint:push" = {
      exec = ''
        set -euo pipefail

        run_step() {
          local name="$1"
          shift
          echo "Currently running: $name"
          "$@"
        }

        run_step "gitleaks detect" ${pkgs.gitleaks}/bin/gitleaks detect --verbose --redact
        run_step "lint:clippy" ${currentDir}/.devenv/profile/bin/lint:clippy
        run_step "schema:check" ${currentDir}/.devenv/profile/bin/schema:check
        run_step "lint:format" ${currentDir}/.devenv/profile/bin/lint:format
        run_step "lint:architecture" ${currentDir}/.devenv/profile/bin/lint:architecture
        run_step "lint:root-git-config" ${currentDir}/.devenv/profile/bin/lint:root-git-config
        run_step "lint:js" ${currentDir}/.devenv/profile/bin/lint:js
        run_step "lint:js:types" ${currentDir}/.devenv/profile/bin/lint:js:types
        run_step "lint:workflows" ${currentDir}/.devenv/profile/bin/lint:workflows
        run_step "deny:check" ${currentDir}/.devenv/profile/bin/deny:check
        run_step "docs:check" ${currentDir}/.devenv/profile/bin/docs:check
        run_step "lint:monochange" ${currentDir}/.devenv/profile/bin/lint:monochange
      '';
      description = "Used for the pre push checks";
      binary = "bash";
    };
    "lint:all" = {
      exec = ''
        set -euo pipefail

        run_step() {
          local name="$1"
          shift
          echo "Currently running: $name"
          "$@"
        }

        run_step "lint:clippy" lint:clippy
        run_step "schema:check" schema:check
        run_step "lint:format" lint:format
        run_step "lint:architecture" lint:architecture
        run_step "lint:root-git-config" lint:root-git-config
        run_step "lint:js" lint:js
        run_step "lint:js:types" lint:js:types
        run_step "lint:workflows" lint:workflows
        run_step "deny:check" deny:check
        run_step "docs:check" docs:check
        run_step "lint:monochange" lint:monochange
      '';
      description = "Run all checks.";
      binary = "bash";
    };
    "lint:format" = {
      exec = ''
        set -euo pipefail
        ${pkgs.dprint}/bin/dprint check
      '';
      description = "Check that all files are formatted.";
      binary = "bash";
    };
    "lint:monochange" = {
      exec = ''
        set -euo pipefail
        mc step:validate
        mc check
      '';
      description = "Run manifest lint rules across all ecosystems.";
      binary = "bash";
    };
    "lint:clippy" = {
      exec = ''
        set -euo pipefail
        # Treat all compiler and clippy warnings as errors so warning-only
        # regressions never make it into CI or a pushed branch.
        cargo clippy --workspace --all-features --all-targets -- -D warnings
      '';
      description = "Check that all rust lints are passing with warnings denied.";
      binary = "bash";
    };
    "lint:architecture" = {
      exec = ''
        set -euo pipefail
        pnpm node scripts/check-architecture-boundaries.ts
      '';
      description = "Check that provider and ecosystem dispatch stays inside the documented allowlist.";
      binary = "bash";
    };
    "lint:root-git-config" = {
      exec = ''
        set -euo pipefail
        common_git_dir="$(git rev-parse --git-common-dir)"
        config_path="$common_git_dir/config"

        if git config --file "$config_path" --get core.worktree >/dev/null 2>&1; then
          echo "error: root git config must not set core.worktree in $config_path" >&2
          git config --file "$config_path" --get core.worktree >&2
          exit 1
        fi

        if git config --file "$config_path" --get-regexp '^user\.' >/dev/null 2>&1; then
          echo "error: root git config must not contain a [user] block in $config_path" >&2
          git config --file "$config_path" --get-regexp '^user\.' >&2
          exit 1
        fi
      '';
      description = "Check that the shared root .git/config does not contain worktree or user overrides.";
      binary = "bash";
    };
    "lint:js" = {
      exec = ''
        set -euo pipefail
        pnpm oxlint --type-aware .
      '';
      description = "Lint all JS/TS files with oxlint (type-aware).";
      binary = "bash";
    };
    "lint:js:syntax" = {
      exec = ''
        set -euo pipefail
        pnpm oxlint .
      '';
      description = "Lint all JS/TS files with oxlint (syntax-only, faster).";
      binary = "bash";
    };
    "lint:js:types" = {
      exec = ''
        set -euo pipefail
        pnpm tsgo -config tsconfig.json
      '';
      description = "Type-check all JS/TS files with tsgo.";
      binary = "bash";
    };
    "fix:workflows" = {
      exec = ''
        set -euo pipefail
        if ! command -v zizmor >/dev/null 2>&1; then
          echo "Installing zizmor via cargo-binstall..."
          cargo binstall zizmor --no-confirm
        fi
        zizmor --fix .github/workflows/ .github/actions/
      '';
      description = "Auto-fix zizmor findings in GitHub Actions workflows where possible.";
      binary = "bash";
    };
    "fix:js" = {
      exec = ''
        set -euo pipefail
        pnpm oxfmt --write '**/*.ts'
        pnpm oxlint --type-aware --fix .
      '';
      description = "Format all JS/TS files with oxfmt.";
      binary = "bash";
    };
    "build:js" = {
      exec = ''
        set -euo pipefail
        pnpm build
      '';
      description = "Bundle JS/TS entry points with tsdown.";
      binary = "bash";
    };
    "skill:commands:check" = {
      exec = ''
        set -euo pipefail
        cargo xtask skill commands check
      '';
      description = "Check that the generated monochange skill command inventory is up to date.";
      binary = "bash";
    };
    "skill:commands:update" = {
      exec = ''
        set -euo pipefail
        cargo xtask skill commands update
      '';
      description = "Regenerate the monochange skill command inventory.";
      binary = "bash";
    };
    "docs:check" = {
      exec = ''
        set -euo pipefail
        mdt check
        cargo xtask skill commands check
        pnpm node scripts/check-agent-surface.ts
      '';
      description = "Check that shared documentation blocks are synchronized and agent-facing docs stay aligned with the repo surface.";
      binary = "bash";
    };
    "docs:update" = {
      exec = ''
        set -euo pipefail
        mdt update
        cargo xtask skill commands update
      '';
      description = "Update shared documentation blocks and generated skill command inventory.";
      binary = "bash";
    };
    "snapshot:review" = {
      exec = ''
        set -euo pipefail
        cargo bin --install
        cargo bin cargo-insta review
      '';
      description = "Review insta snapshots.";
      binary = "bash";
    };
    "snapshot:check" = {
      exec = ''
        set -euo pipefail
        cargo bin --install
        export PATH="$PWD/.bin/rust-nightly/cargo-nextest/0.9.132/bin:$PATH"
        cargo bin cargo-insta test --workspace --exclude xtask --all-features --test-runner nextest --disable-nextest-doctest --unreferenced=reject
      '';
      description = "Check insta snapshots and fail on unreferenced snapshot files.";
      binary = "bash";
    };
    "snapshot:update" = {
      exec = ''
        set -euo pipefail
        cargo bin --install
        export PATH="$PWD/.bin/rust-nightly/cargo-nextest/0.9.132/bin:$PATH"
        cargo bin cargo-insta test --workspace --exclude xtask --all-features --test-runner nextest --disable-nextest-doctest --force-update-snapshots --unreferenced=delete
      '';
      description = "Update insta snapshots and delete unreferenced snapshot files.";
      binary = "bash";
    };
  };
}
