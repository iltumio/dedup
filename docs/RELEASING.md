# Releasing

Releases are automated with [Knope](https://knope.tech) and driven by
[Conventional Commits](https://www.conventionalcommits.org). You never bump versions
or edit `CHANGELOG.md` by hand.

## How a release happens

1. **Land work on `main`** using conventional commit messages
   (`feat:`, `fix:`, `perf:`, `feat!:` / `BREAKING CHANGE:` footer, ...).
2. **A Release PR opens automatically.** On every push to `main`,
   [`prepare_release.yml`](../.github/workflows/prepare_release.yml) runs
   `knope prepare-release`, which computes the next SemVer version, updates the five
   versioned files and `CHANGELOG.md`, and opens/updates a PR from the `release` branch.
3. **Merge the Release PR** when you're ready to ship. The merge triggers
   [`release.yml`](../.github/workflows/release.yml): it builds the CLI, the Tauri
   bundles, and the Arch package, then runs `knope release` to tag `v$version`, publish
   the GitHub release with the changelog notes, and attach every built artifact.

Version numbers are derived from the commits since the last tag:
`fix:`/`perf:` → patch, `feat:` → minor, `!` or `BREAKING CHANGE:` → major.

## Versioned files

All of these are kept in lockstep (configured in [`knope.toml`](../knope.toml)):

- `crates/dedup-core/Cargo.toml`
- `crates/dedup-cli/Cargo.toml`
- `app/src-tauri/Cargo.toml`
- `app/src-tauri/tauri.conf.json`
- `app/package.json`

## One-time setup

- **Optional:** add a `RELEASE_PAT` repository secret (a fine-grained PAT with
  `contents: write` and `pull-requests: write`). It lets the Release PR trigger CI
  checks. Without it the workflow falls back to `GITHUB_TOKEN` — the PR still opens,
  it just won't kick off other workflows.

## Local preview

```sh
just changelog-preview   # knope prepare-release --dry-run
```

Shows the next version and changelog entries without changing anything.
