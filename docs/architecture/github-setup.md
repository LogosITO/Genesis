# Manual GitHub setup checklist

Nothing in this list has been performed by local file creation. A repository administrator must:

- [ ] Confirm the final project name, public repository owner/name, and license attribution before publishing.
- [ ] Make the repository public only after reviewing source, history, secrets, and community contact routes.
- [ ] Set default branch to `main`; enable a ruleset requiring PR review and passing `CI / commit-title`, `CI / rust (ubuntu-latest)`, `CI / rust (windows-latest)`, and `Documentation and site / build` checks; prevent direct pushes and force pushes as appropriate for maintainers. Configure squash merge to use the PR title as the commit subject.
- [ ] Enable Dependabot alerts, security updates, dependency graph, code scanning, and private vulnerability reporting where available.
- [ ] Set a private contact route for security and conduct reports; update `SECURITY.md` and `CODE_OF_CONDUCT.md`.
- [ ] Enable GitHub Discussions if maintainers can moderate it.
- [ ] In **Settings → Pages → Build and deployment**, verify **GitHub Actions** is the source. The first public run built and uploaded the site but `deploy-pages` returned HTTP 404. A later public deployment record for the same `de48627` commit reports success, and `https://logosito.github.io/Genesis/` serves the Astro site with `/Genesis/` asset paths. The Pages source setting is not visible through the unauthenticated API, so an administrator should confirm it and monitor the next `main` deployment. If 404 recurs, inspect the deploy job log and organization Pages policy.
- [ ] In **Settings → Environments → github-pages**, retain the deployment branch rule for `main`. The public environment API reported a `main` branch policy on 2026-10-09. The workflow already grants only its `deploy` job `pages: write` and `id-token: write`; pull requests only build and check the site.
- [ ] After the Linux CI fix lands, rerun or synchronize the six open Dependabot PRs. Their first runs all passed `commit-title` and site build, but failed the same Ubuntu Clippy job; Windows CI passed. Review action major-version PRs individually before merging. Do not relax required checks or auto-merge them.
- [ ] Review scheduled `security.yml` results and investigate new advisories. The current lockfile overrides a transitive CSS parser to its patched version; verify the override remains compatible after dependency updates.
- [ ] Review the [release policy](releases.md) and its independent library versions. Create a GitHub App installed only on this repository with repository **Contents: read/write**, **Issues: read/write** (Release Please labels and comments), and **Pull requests: read/write**. Set repository variable `RELEASE_APP_ID` and repository secret `RELEASE_APP_PRIVATE_KEY` for PR preparation. Do not use a personal token or the default `GITHUB_TOKEN` for generated release PRs.
- [ ] Create a `release` environment with required maintainer reviewers and restrict its deployment branches to `main`. Store `RELEASE_PUBLISH_APP_PRIVATE_KEY` as an **environment secret only**, after protection is configured. The publish job cannot mint a token without it; do not put this secret at repository scope.
- [ ] Confirm a generated Release Please PR actually triggers and passes the required PR checks. Review its versions, changelogs, dependent crate bumps, and `Cargo.lock` before merging. Test manual publication only when a real release is approved; check tags and GitHub Releases afterward.
- [ ] Decide supported release artifacts, signing/provenance, and a crates.io publication policy separately. This workflow creates source tags and GitHub Releases only.

No `CODEOWNERS` exists because owners are not yet known. No nightly workflow exists because there is no extended test corpus or benchmark gate. GitHub branch settings, Actions execution, and Pages deployment cannot be confirmed locally.
