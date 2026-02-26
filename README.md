<p align="center">
  <img src="assets/conman.svg" alt="Conman" width="200" />
</p>

# Conman

Conman is a Git-backed configuration manager backend (Rust, API-first).

It models and orchestrates:

- apps (Git repositories)
- workspaces (user editing branches)
- changesets (reviewable proposals)
- releases (queue-first, publishable artifacts)
- deployments across configurable environments
- runtime profiles (URL, env vars, secrets, DB/data/migration settings)

## Status

This repository is in early implementation stage. The v1 scope and execution
plan are documented and continuously refined in `docs/`.

## Source of Truth

- Scope: [docs/conman-v1-scope.md](docs/conman-v1-scope.md)
- Implementation guide: [docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md)
- Backlog: [docs/conman-v1-backlog.md](docs/conman-v1-backlog.md)
- Runtime profiles draft: [docs/runtime-profiles-draft.md](docs/runtime-profiles-draft.md)
- Tenant/repo/app-surface model: [docs/tenant-repo-app-surface-model.md](docs/tenant-repo-app-surface-model.md)
- Tenant/repo/app-surface implementation plan: [docs/tenant-repo-app-surface-implementation-plan.md](docs/tenant-repo-app-surface-implementation-plan.md)
- Epics: [docs/epics/](docs/epics/)

## Repository Layout

- `src/`: Rust binary entrypoint (currently minimal bootstrap).
- `docs/`: scope, backlog, implementation guide, epics, and published site content.
- `scripts/`: docs build/publish scripts.

## Local Development

Prerequisites:

- Rust toolchain (`cargo`)
- `pandoc` (for docs-to-HTML site generation)

Bootstrap local env:

```bash
cp .env.example .env
```

Run:

```bash
cargo run
```

API docs while running locally:

```bash
open http://127.0.0.1:3000/api/docs
```

Bootstrap first login user:

```bash
cargo run -- bootstrap-admin admin@example.com "Admin User" "AdminPassw0rd!!"
```

Manual end-to-end API testing sequence:

- [docs/manual-api-testing-guide.md](docs/manual-api-testing-guide.md)

Build:

```bash
cargo build
```

## Docs Site

Build static docs HTML:

```bash
./scripts/build-docs-site.sh
```

Publish with Wrangler (target production):

```bash
CLOUDFLARE_PAGES_BRANCH=main ./scripts/publish-docs-site.sh
```

Optional explicit project:

```bash
CLOUDFLARE_PAGES_BRANCH=main ./scripts/publish-docs-site.sh conman-docs
```

## Delivery Gate

Run the end-to-end plan completion gate:

```bash
CONMAN_SECRETS_MASTER_KEY='<prod-key>' ./tests/ops/run_plan_completion_gate.sh --strict
```

This verifies milestone/checklist completion, runs tests + clippy, rebuilds
the docs site, and records a summary under `tests/ops/results/`.

## CI/CD

- PRs and pushes to `master`/`main` run `.github/workflows/ci.yml` and execute:
  - `./tests/ops/run_plan_completion_gate.sh --strict`
- Docs changes run `.github/workflows/docs-pages.yml`:
  - Always builds the docs artifact.
  - Deploys to Cloudflare Pages production branch `main` when
    `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` are configured in repo
    secrets.

## Notes

- Git operations are planned behind an internal adapter boundary, with
  `gitaly-rs` as the primary backend target.
- Metadata, workflow state, and audit trail are planned in MongoDB; Git remains
  source of truth for files/history.
