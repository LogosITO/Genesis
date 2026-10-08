# Manual GitHub setup checklist

Nothing in this list has been performed by local file creation. A repository administrator must:

- [ ] Confirm the final project name, public repository owner/name, and license attribution before publishing.
- [ ] Make the repository public only after reviewing source, history, secrets, and community contact routes.
- [ ] Set default branch to `main`; enable a ruleset requiring PR review and passing `CI / rust (ubuntu-latest)`, `CI / rust (windows-latest)`, and `Documentation and site / build` checks; prevent direct pushes and force pushes as appropriate for maintainers.
- [ ] Enable Dependabot alerts, security updates, dependency graph, code scanning, and private vulnerability reporting where available.
- [ ] Set a private contact route for security and conduct reports; update `SECURITY.md` and `CODE_OF_CONDUCT.md`.
- [ ] Enable GitHub Discussions if maintainers can moderate it.
- [ ] Set Pages source to GitHub Actions. The `docs.yml` deploy job publishes only a successful `main` build; verify the Pages URL and repository base path after first run.
- [ ] Review scheduled `security.yml` results and investigate new advisories. The current lockfile overrides a transitive CSS parser to its patched version; verify the override remains compatible after dependency updates.
- [ ] Define release versioning, supported platforms, artifact contents, signing/provenance, and a maintainer approval path. Only then add a release workflow; do not publish every merge.

No `CODEOWNERS` exists because owners are not yet known. No nightly workflow exists because there is no extended test corpus or benchmark gate. GitHub branch settings, Actions execution, and Pages deployment cannot be confirmed locally.
