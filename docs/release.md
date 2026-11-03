# Manual fork releases

This fork publishes the `document-graphics-mermaid` library from `fork-release`.
Rust imports remain `mermaid_rs_renderer`. The upstream `master` and independent
`fix/` branches retain upstream package metadata for contribution and syncing.
The fork version is independent of upstream's version.

The inherited release workflow is removed on this branch. Ordinary CI checks
builds, tests and packages; it does not publish crates, CLI binaries, Homebrew
formulae or AUR packages. Publishing automation can be added if the fork becomes
long-lived.

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
