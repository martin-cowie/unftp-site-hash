# Releasing

Releases are made by hand. Set the version being released, then work through the steps in order.

```sh
VERSION=X.Y.Z
```

## 1. Prepare the release commit

- Set `version` in `Cargo.toml` to the value of `$VERSION` (e.g. `0.2.0`, no `v` prefix), then run `cargo build` to update `Cargo.lock`.
- In `CHANGELOG.md`, rename `## Unreleased` to `## $VERSION — YYYY-MM-DD`, writing out the value (e.g. `## 0.2.0 — 2026-10-13`).

```sh
git commit -am "Release $VERSION." && git push
```

## 2. Wait for CI to pass

```sh
gh run watch
```

## 3. Check the package

```sh
cargo package --list
cargo publish --dry-run
```

Check the file list contains nothing unexpected. Run without `--allow-dirty`, so that uncommitted changes fail the check.

## 4. Publish to crates.io

```sh
cargo login      # once per machine, with a token from https://crates.io/settings/tokens
cargo publish
```

A published version cannot be deleted, only yanked. Publish before tagging, so that a failed publish leaves no tag behind.

## 5. Tag the release

```sh
git tag "v$VERSION" && git push --tags
```

## 6. Create the GitHub release

```sh
gh release create "v$VERSION" --title "v$VERSION" \
  --notes-file <(sed -n "/^## $VERSION/,/^## /p" CHANGELOG.md | sed '1d;/^## /d')
```

## 7. Open the next cycle

Add a new `## Unreleased` heading at the top of `CHANGELOG.md`.

docs.rs builds the documentation automatically within a few minutes of publishing; check it at <https://docs.rs/unftp-site-hash>.
