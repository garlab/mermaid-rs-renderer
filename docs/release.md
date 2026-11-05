# Manual fork releases

This fork publishes the `document-graphics-mermaid` library from `fork-release`.
Rust imports remain `mermaid_rs_renderer`. The upstream `master` and independent
`fix/` branches retain upstream package metadata for contribution and syncing.
The fork version is independent of upstream's version.

The inherited release workflow is removed on this branch. Ordinary CI checks
builds, tests and packages; it does not publish crates, CLI binaries, Homebrew
formulae or AUR packages. Publishing automation can be added if the fork becomes
long-lived.

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

The release remains unpublished. Push `fork-release` and wait for the platform
CI before publishing. Publish this backend before document-graphics 0.1.7, then
validate that library against the registry package with its local override
removed. Create the backend tag only after successful publication, as below.

## Prepare a release

1. Integrate the reviewed fix branches into `fork-release`. Keep the package
   rename and fork release configuration out of upstream pull requests.
2. Set the new fork version in `Cargo.toml`. Update the README and crate-level
   dependency examples, refresh `Cargo.lock`, and record the included fixes.
3. Run the checks below, commit, and push `fork-release`. Wait for its CI to pass
   on the supported platforms before publishing.

```sh
python3 scripts/check_repo.py
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

Inspect the archive's source, tests, fixture license and metadata. Recheck
document-graphics and its consumers before changing the renderer version.
Local dependency overrides belong in untracked Cargo configuration.

## Publish from the CLI

Use normal Cargo registry authentication (`cargo login` or a configured
credential provider). Never commit registry tokens. The crate name must be
`document-graphics-mermaid`, and its repository metadata must identify this fork.

From the clean, reviewed `fork-release` commit:

```sh
git diff --exit-code
git diff --cached --exit-code
cargo publish -p document-graphics-mermaid --dry-run --locked
cargo publish -p document-graphics-mermaid --locked
```

Verify that the expected version is available on crates.io. Tag the exact
published commit, using the fork's package name to distinguish its releases:

```sh
version="$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
git tag -a "document-graphics-mermaid-v${version}" -m "Release document-graphics-mermaid ${version}"
git push origin "document-graphics-mermaid-v${version}"
```

Then update document-graphics to consume the published fork and validate without
local overrides before releasing the library. Update consuming applications only
after that library release is available.
Once upstream ships equivalent APIs and fixes, switch back to the upstream crate.
