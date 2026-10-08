# Release governance

**Status: configured locally; no hosted release has been tested.** This policy covers all eight Cargo workspace packages: six library crates and two examples. They have independent SemVer versions. The private site is not a release component. A GitHub Release is a source-code milestone; this workflow does not publish to crates.io, npm, Pages, or a binary download channel. Both examples have `publish = false`.

## Version rules

Each Cargo package starts at the existing `0.1.0` manifest value. That value is an **unreleased baseline**, not evidence that a `0.1.0` tag or GitHub Release exists. The first Release Please scan starts after commit `901fd1d0ea2f602fe96e520f806a560d191bda09`; the `bootstrap-sha` can be removed after the first release PR is merged.

Use Conventional Commit subjects for squash commits on `main`. The PR title check enforces the shape `type(scope)!: summary`, with optional scope and `!`. Prefer the affected package as scope, but Release Please assigns commits by changed paths, not by scope text. If a change spans packages, inspect every proposed version. A change only to root documentation or the site does not produce a Cargo release.

| Change in a package | Before 1.0 | From 1.0 | Example |
| --- | --- | --- | --- |
| `fix:` | patch | patch | `0.1.0 → 0.1.1` |
| `feat:` | minor | minor | `0.1.0 → 0.2.0` |
| `type!:` or `BREAKING CHANGE:` | minor | major | `0.1.0 → 0.2.0` |

`perf`, `refactor`, `docs`, `test`, `build`, `ci`, and `chore` are allowed PR title types; do not assume they trigger a release by themselves. Mark any incompatible public contract with `!` and describe migration, even before 1.0. Release Please can patch-bump any dependent workspace package, including examples, when a dependency changes; review these cascades and `Cargo.lock`. The generated package changelogs are part of the release PR. Component tags use `<package-name>-v<version>`, for example `spatial-math-v0.2.0`.

## Maintainer procedure

1. Merge ordinary PRs through protected `main` using squash commits whose subjects match their PR titles. Review changes to public API, Rust minimum version, mathematical contracts, and platform support as potential breaking changes.
2. After a push to `main`, the `prepare` job proposes or updates a combined Release Please PR. It cannot create tags or GitHub Releases. The GitHub App token lets that PR trigger the normal `pull_request` checks.
3. Review the generated versions, changelogs, dependent crate bumps, `Cargo.lock`, and any proposed release notes. Require the usual CI and documentation checks plus a maintainer review. Correct a mistaken commit classification before merging; do not blindly merge a generated PR.
4. Merge the release PR. It changes version files and changelogs on `main` but does not publish. Run `Release governance` with **Run workflow** on `main` only when ready to create the tags and GitHub Releases. The `publish` job uses the protected `release` environment. Check the resulting tags and release contents before merging another release PR. Do not run the manual publish job for an unreviewed or unmerged release PR.

The workflow is intentionally inert until the GitHub App credentials are installed. The repository administrator must complete the [GitHub setup checklist](github-setup.md) before enabling it. Only trusted `main` pushes and manual dispatch from `main` receive the App token; pull requests never receive it. A skipped job means no release preparation occurred. The default `GITHUB_TOKEN` is unsuitable here because PRs it creates do not normally trigger required CI checks.

## Deferred decisions

Crates currently use path-only internal dependencies, so this is not a crates.io publication process. Before publishing there, choose package names and ownership, add publishable version requirements and metadata, test `cargo package`, and define provenance and support policy. The `first-light` binary has no supported distribution artifact yet. Avoid treating experimental GPU behavior or numerical approximations as stable solely because a version or tag exists.
