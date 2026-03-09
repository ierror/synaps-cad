# Releasing

This project uses [cargo-release](https://github.com/crate-ci/cargo-release) to automate releases.

## First-time setup

```bash
cargo install cargo-release
cargo login
```

The `cargo login` command requires an API token from https://crates.io/settings/tokens.

## Releasing

Dry-run (no changes made):

```bash
cargo release patch   # or minor, major
```

Execute the release:

```bash
cargo release patch --execute
```

This will:

1. Run `cargo clippy -- -D warnings` (pre-release hook — aborts if there are warnings)
2. Bump the version in `Cargo.toml` (the only place the version is hardcoded)
3. Commit the version bump
4. Create a git tag (`vX.Y.Z`)
5. Push the commit and tag to GitHub

> **Note:** `cargo release` handles the `Cargo.toml` version automatically. The macOS `Info.plist` uses a `${APP_VERSION}` placeholder that CI replaces with the version from the git tag — no manual version bumping needed beyond `cargo release`.

## What happens next

Once the tag is pushed, the GitHub Actions release workflow (`.github/workflows/release.yml`) automatically:

1. Builds release binaries for all supported platforms:
   - Linux x86_64
   - macOS Apple Silicon (aarch64)
   - macOS Intel (x86_64)
   - Windows x86_64
2. Packages them as `.tar.gz` (Linux/macOS) or `.zip` (Windows)
3. Injects the version from the git tag into `Info.plist` for macOS `.app` bundles
4. Creates a GitHub Release with changelog notes and all artifacts attached

You can monitor the workflow run at **Actions > Release** in the GitHub repository.
