# Release and Homebrew publishing

Tags matching `v*` trigger `.github/workflows/release.yml`. The workflow:

1. Confirms the tag matches the version in `Cargo.toml`.
2. Runs formatting, Clippy, and unit tests.
3. Builds Intel and Apple Silicon binaries and combines them into one universal macOS binary.
4. Publishes the archive and SHA-256 checksum to GitHub Releases.
5. Updates `Formula/dutis.rb` in `tsonglew/homebrew-tap`.

## One-time repository setup

Create a fine-grained GitHub personal access token with read/write access to the contents of `tsonglew/homebrew-tap`. Add it to the `tsonglew/dutis` repository as an Actions secret named:

```text
HOMEBREW_TAP_TOKEN
```

Protect the `master` branch and require the `Format, lint, and test` and architecture build checks before merging.

## Create a release

Update the version in `Cargo.toml`, merge the change into `master`, then create and push the matching tag:

```bash
git tag v2.4.0
git push origin v2.4.0
```

The release workflow can also be rerun manually for an existing tag using the **Run workflow** button.

After the workflow succeeds, verify installation in a clean environment:

```bash
brew update
brew install tsonglew/tap/dutis
dutis --help
```
