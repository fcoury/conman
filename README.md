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
- Epics: [docs/epics/](docs/epics/)

## Repository Layout

- `src/`: Rust binary entrypoint (currently minimal bootstrap).
- `docs/`: scope, backlog, implementation guide, epics, and published site content.
- `scripts/`: docs build/publish scripts.

## Local Development

Prerequisites:

- Rust toolchain (`cargo`)
- `pandoc` (for docs-to-HTML site generation)

Run:

```bash
cargo run
```

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

## Notes

- Git operations are planned behind an internal adapter boundary, with
  `gitaly-rs` as the primary backend target.
- Metadata, workflow state, and audit trail are planned in MongoDB; Git remains
  source of truth for files/history.
