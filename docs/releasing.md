# Release and Homebrew publishing

Merges into `master` trigger `.github/workflows/release.yml`. The workflow:

1. Reads the version from `Cargo.toml` and creates the matching `vX.Y.Z` tag if it does not exist.
2. Runs formatting, Clippy, and unit tests.
3. Builds Intel and Apple Silicon binaries, combines `dutis` and
   `dutis-event-http` into universal macOS binaries, and packages the bundled
   Dutis agent skill.
4. Publishes the archive and SHA-256 checksum to GitHub Releases.
5. Updates `Formula/dutis.rb` in `tsonglew/homebrew-tap`.

If the version tag already exists, the workflow exits successfully without republishing. This makes ordinary merges safe: a release happens only after the package version is bumped.

## One-time repository setup

Create a fine-grained GitHub personal access token with read/write access to the contents of `tsonglew/homebrew-tap`. Add it to the `tsonglew/dutis` repository as an Actions secret named:

```text
HOMEBREW_TAP_TOKEN
```

Protect the `master` branch and require the `Format, lint, and test` and architecture build checks before merging.

## Create a release

Update the version in `Cargo.toml` and merge the change into `master`:

```toml
[package]
version = "2.4.0"
```

The merge automatically creates `v2.4.0` and completes the release pipeline. Do not create the tag manually for normal releases.

The release workflow can still be rerun manually for an existing tag using the **Run workflow** button.

After the workflow succeeds, verify installation in a clean environment:

```bash
brew update
brew install tsonglew/tap/dutis
dutis --help
dutis-event-http --version
test -f "$(brew --prefix dutis)/share/dutis/skills/dutis/SKILL.md"
```
