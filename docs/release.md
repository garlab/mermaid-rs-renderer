# Fork releases

This fork publishes the `document-graphics-mermaid` library from `main`.
Rust imports remain `mermaid_rs_renderer`. The upstream `master` and independent
`fix/` and `perf/` branches retain upstream package metadata for contribution and
syncing. The fork version is independent of upstream's version.

A `document-graphics-mermaid-v<version>` tag triggers the release workflow. It
checks the package name and version, requires the tagged commit to be on
`origin/main`, then runs the complete reusable CI workflow: platform tests,
feature checks, Clippy, Rust 1.90, Nix, layout quality and package verification.
Only after these pass can the `release` environment publish the crate using a
short-lived crates.io token. Ordinary branch and pull-request CI has read-only
permissions. This workflow publishes the library crate; CLI binaries, Homebrew
and AUR distribution remain upstream concerns.

## One-time setup

1. Rename the GitHub repository to `garlab/document-graphics-mermaid`.
2. After the GitHub rename, update the local remote, push the maintained branch
   and establish its tracking branch:

   ```sh
   git remote set-url origin git@github.com:garlab/document-graphics-mermaid.git
   git push -u origin main
   ```

   Set `main` as the default branch on GitHub. Retain `master` for upstream
   syncing. The local checkout directory can keep its existing name. Consumer path
   overrides only need updating if that directory is renamed separately.
3. Create the GitHub Actions environment `release`. If deployment restrictions
   are enabled, allow tags matching `document-graphics-mermaid-v*`.
4. In the crate's crates.io settings, add a GitHub Trusted Publisher with owner
   `garlab`, repository `document-graphics-mermaid`, workflow `release.yml` and
   environment `release`. See the [Trusted Publishing guide](https://crates.io/docs/trusted-publishing).
   No stored Cargo API token is needed by the workflow.
5. Enable Actions for the fork and confirm branch CI passes on `main` before
   pushing a release tag.

## Version 0.2.0

This release adds validated native Gantt scheduling: Gregorian dates, clock and
date-time formats down to milliseconds, exact durations, dependencies, combined
status tags, milestones and weekend/holiday exclusions. Invalid or unsupported
schedules return errors so consumers can retain source. Scheduling and ticks are
bounded; no dependency, JavaScript runtime or clock/timezone lookup is added.
See the [Gantt contract](gantt.md) for coverage and limits.

The minor version marks changes to the public Gantt IR: independent status flags,
an explicit task end, graph date/calendar/axis fields, and `f64` layout times in
civil-day units. Callers constructing those structs must adapt; the string-based
parse/render entry points retain their signatures.

Local release validation on 2026-11-05 (macOS, Apple Silicon) passed the full and
minimal feature suites, embedded-font and documentation tests, all-feature
Clippy, Rust 1.90 checks, repository checks, package verification and a publishing
dry run. The archive contains the new time module and tests, fixture/font license
notices, versioned dependencies and no local paths. document-graphics and both
consumer integration paths were checked, with the GUI consumer tested in an
isolated checkout updated to the new library version.

Version 0.2.0 is already published on crates.io. The automation applies to
subsequent releases. A tag for an existing version still runs validation and CI,
then skips authentication and publication. A failed registry lookup stops the
job; only a confirmed HTTP 404 permits a new upload.

## Prepare a release

1. Integrate reviewed changes into `main`. Keep the package rename and fork
   release configuration out of upstream pull requests.
2. Set the new fork version in `Cargo.toml`. Update the README and crate-level
   dependency examples, refresh `Cargo.lock`, and record the included fixes.
3. Run the checks below, inspect the package, commit, and push `main`. Wait for
   its CI to pass on the supported platforms.

```sh
python3 scripts/check_repo.py
python3 -m unittest discover -s .github/tests -v
cargo fmt -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features --lib --bins --tests --examples
cargo test --locked --no-default-features --lib --test embedded_fonts
cargo test --locked --doc --all-features
cargo +1.90.0 check --locked --all-targets --all-features
cargo package --locked
cargo package --list --locked
cargo publish -p document-graphics-mermaid --dry-run --locked
```

Inspect the archive's source, tests, fixture licenses and metadata. Recheck
document-graphics and its consumers before changing the renderer version.
Local dependency overrides belong in untracked Cargo configuration.

## Publish a tag

From the clean, reviewed `main` commit, derive the tag from Cargo metadata:

```sh
git diff --exit-code
git diff --cached --exit-code
version="$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
git tag -a "document-graphics-mermaid-v${version}" -m "Release document-graphics-mermaid ${version}"
git push origin "document-graphics-mermaid-v${version}"
```

Wait for the Release workflow and verify the expected version on crates.io.
Package-prefixed tags remain distinct from upstream's `v*` tags. Workflow runs
are serialized, and re-running a successful release skips its published version.
If validation fails because `main` has not been pushed, push it and re-run the
workflow; do not move a tag after publication.

Then update document-graphics to consume the published fork and validate without
local overrides before releasing that library. Update consuming applications only
after the library release is available.

## Manual fallback

If automation is unavailable, use normal Cargo registry authentication and run
the same validation from the clean release commit:

```sh
cargo publish -p document-graphics-mermaid --dry-run --locked
cargo publish -p document-graphics-mermaid --locked
```

Tag the exact published commit using the commands above. The workflow will
recognize the existing version. Never commit registry credentials.
