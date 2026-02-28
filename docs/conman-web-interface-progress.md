# Conman Web Interface Progress

## Milestones
- [x] M1 Backend foundation for single-repo UI context
- [x] M2 Frontend app scaffold (`web/`) with shared layout/auth/runtime plumbing
- [x] M3 Setup wizard + repo binding workflow
- [x] M4 Core workflow screens (workspaces/files, changesets, releases, deployments)
- [x] M5 Supporting screens (runtime/envs, temp envs, jobs, apps, members, notifications, settings)
- [x] M6 Backend-served SPA routes (`/app`, `/app/*`) and frontend E2E/unit tests baseline
- [x] M7 Route migration: serve web UI from root (`/`) with API isolation
- [x] M8 Setup wizard hardening (explicit selection, scoped repo selection, post-bind redirect)
- [x] M9 Explicit onboarding flow (`instance` first) with `<app>--<instance>.dxflow-app.com`
- [x] M10 Role-first dashboard UX sweep (author/reviewer/release/admin flows)

## Completed
### M1 Backend foundation for single-repo UI context
- Added `UiConfig` domain model in `conman-core`.
- Added `UiConfigRepo` Mongo repository in `conman-db`.
- Added `GET /api/repo` and `PATCH /api/repo` handlers.
- Added `GET /api/teams/{teamId}/invites` handler + DB list method.
- Wired router/auth/openapi + Mongo index bootstrap for new repo.
- Verified with `cargo check`.

### M2-M6 Frontend + app serving implementation
- Scaffolded `web/` React + Vite + TypeScript app with `pnpm`.
- Added Tailwind v4 theme/token baseline and component primitives.
- Implemented auth lifecycle screens:
  - login, signup, forgot/reset password, accept invite.
- Implemented protected routing and single-repo context guard:
  - redirects unbound instances to setup.
  - shows access denied on repo membership mismatch.
- Implemented setup workflow:
  - bind existing repo, create team, create repo, create first app, bind repo.
- Implemented module pages:
  - Workspaces + Monaco editor + tree filter + save/delete/checkpoint/sync/reset.
  - Changesets review pipeline actions and diff/comment utilities.
  - Releases actions (create/set/reorder/assemble/publish).
  - Deployments actions (deploy/promote/rollback).
  - Runtime/environments management + secret reveal action.
  - Temp env lifecycle actions.
  - Jobs dashboard with auto-refresh for active states.
  - Apps, Members/Invites, Notifications, Settings (rebind).
- Added backend `/app` + `/app/*` static serving with SPA fallback to `web/dist`.
- Added frontend test baseline:
  - Vitest setup and auth hook unit test.
  - Playwright config + smoke test file.

### M10 Role-first dashboard UX sweep
- Reorganized left navigation around actual workflows:
  - `Build` (Draft Changes, Changesets)
  - `Release` (Releases, Deployments, Temp Envs)
  - `Operations` (Runtime, Jobs)
  - `Administration` (Apps, Members, Notifications, Settings)
- Added role-aware nav visibility and route-level guards:
  - `config_manager+` for release/runtime areas.
  - `admin+` for app/member/settings administration.
- Refactored dashboard pages to reduce API-console feel:
  - Added role-scope context cards on pages.
  - Replaced always-visible raw JSON dumps with collapsible `Advanced payload` panels.
  - Clarified sequential actions (draft -> changeset -> review -> release -> deploy).
- Added startup cleanup for legacy app indexes to avoid onboarding/app-create failures on older local Mongo states.
- Made UI binding user-scoped (`ui_config._id = user_id`) with automatic bind-to-first-accessible repo fallback.
  - This unblocks invited reviewers/config managers from being forced into setup or blocked by another user's binding.
  - `PATCH /api/repo` now requires `member` access on target repo (instead of `admin`) for per-user binding.

## Verification
- `cargo check` (workspace): ✅
- `pnpm --dir web lint`: ✅ (warnings only)
- `pnpm --dir web test`: ✅
- `pnpm --dir web build`: ✅
- `GET /` serves login shell with built assets from `web/dist`: ✅
- `GET /api/nonexistent` returns JSON 404 envelope (not SPA fallback): ✅
- `agent-browser` console on `/` and `/app` is clean: ✅
- Setup wizard checks (agent-browser + Playwright MCP): ✅
  - Team step requires explicit team selection.
  - Repo step scopes list to selected team and requires explicit selection.
  - Bind step requires explicit repo selection; no auto-first fallback.
  - Completion CTA now lands on `/workspaces`.
- Explicit onboarding checks (agent-browser): ✅
  - Signup no longer auto-creates a repo; setup starts at instance naming.
  - Instance creation issues a refreshed token and proceeds to first-app step.
  - App URL preview/creation follows `<app>--<instance>.dxflow-app.com`.
  - Final setup action binds created instance and lands on `/workspaces`.
- Dashboard UX sweep checks (agent-browser): ✅
  - Post-signup onboarding still completes successfully.
  - First-step instance-name conflicts now auto-suggest next available name/slug.
  - All dashboard routes render with role-scope guidance and simplified sections.
  - Sidebar now emphasizes main author path (`Draft Changes` -> `Changesets`).
  - Role visibility + route access validated with reviewer/config-manager sessions.

## Notes
- `.aidocs` is locally ignored via global gitignore; this progress log is intentionally local unless ignore rules change.
- Current ESLint output includes 2 non-blocking `react-hooks/exhaustive-deps` warnings in setup page memo helpers.
- Scope update applied after implementation feedback: web app now mounts at `/`
  instead of `/app`; backend preserves `/api/*` behavior and 404 semantics.
