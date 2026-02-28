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
- [x] M11 Post-onboarding product realignment (task-first UX for build/review/release)

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

### M11 Post-onboarding product realignment (completed)
- Added delivery docs for scope and execution tracking:
  - `docs/post-onboarding-ux-plan.md`
  - `docs/post-onboarding-ux-backlog.md`
- Implemented Phase 1 first pass on core author/reviewer screens:
  - `Draft Changes` rewritten as a guided task flow:
    1) select/create workspace, 2) edit files, 3) create changeset.
  - Added inline changeset creation from selected workspace (without switching pages first).
  - Improved editor state feedback (`Unsaved changes` vs `Saved`) and action success states.
  - Moved raw payload inspection behind explicit `Advanced payload` disclosure.
  - `Changesets` rewritten with list/detail workflow:
    - status filter chips with counts,
    - selected changeset detail panel,
    - context-aware primary actions (submit/resubmit/queue/move-to-draft),
    - reviewer action panel + semantic-first diff loading.
  - Added page-level status feedback cards for action outcomes.
  - Refactored `Releases` into a release-composer UX:
    - queued changeset selection checkboxes,
    - composition order controls (up/down),
    - explicit save-selection/save-order/assemble/publish flow.
  - Refactored `Deployments` into pipeline-oriented UX:
    - environment snapshot cards with latest state,
    - action form centered on env + release + action,
    - filtered history list with stronger status readability.
  - Refactored `Runtime` page toward typed forms:
    - profile create/update with common typed fields,
    - environment-chain editor with ordering controls,
    - advanced JSON patch moved behind explicit disclosure.
  - Refactored `Temp Environments` page into preview-centered workflow:
    - source selection via workspace/changeset dropdowns,
    - selected-environment actions (extend/undo/delete) without manual IDs,
    - list cards emphasize state, expiry, and preview URL.
  - Updated navigation/route access so preview environments are visible in `Build`
    as `Preview Envs` (member-accessible), aligning with author-first validation flow.
  - Added release impact visibility:
    - semantic-diff-based impact summary with changed-path preview and author/state stats.
  - Expanded deployment history usability:
    - environment + state filters, search, selectable history rows, detail panel.
  - Cleaned up admin/member operations:
    - Members page now centers invite-by-email and member-role update flow.
    - Settings rebind flow now uses team/instance selectors instead of raw ID-first UX.
  - Accessibility pass for core controls:
    - added reusable `label` support to `Select` and `Textarea`,
    - applied labels and `aria-live` status announcements on workflow pages,
    - marked decorative sidebar icons with `aria-hidden`.
  - Added regression coverage for redesigned workflows via new utility tests:
    - changeset state/filter helpers,
    - workspace path navigation helper,
    - release impact summarization helper,
    - deployment history filter/count helpers.
  - Added Playwright workflow coverage with API-mocked role journeys:
    - release impact summary flow,
    - deployment history filtering/detail flow,
    - members + settings guided admin flow.
  - Updated Playwright config with `webServer` startup to make `test:e2e` self-contained.

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
- M11 Phase 1 checks:
  - `pnpm --dir web lint`: ✅ (existing warning remains in setup step dependencies)
  - `pnpm --dir web test -- --run`: ✅
  - `pnpm --dir web build`: ✅
- M11 Phase 2-4 checks:
  - `pnpm --dir web lint`: ✅ (same existing setup warning)
  - `pnpm --dir web test -- --run`: ✅ (5 files, 8 tests)
  - `pnpm --dir web build`: ✅
  - `pnpm --dir web test:e2e`: ✅ (4 tests passing)

## Notes
- `.aidocs` is locally ignored via global gitignore; this progress log is intentionally local unless ignore rules change.
- Current ESLint output includes 1 non-blocking `react-hooks/exhaustive-deps` warning in setup memo dependencies.
- Scope update applied after implementation feedback: web app now mounts at `/`
  instead of `/app`; backend preserves `/api/*` behavior and 404 semantics.
- Realignment planning docs added:
  - `docs/post-onboarding-ux-plan.md`
  - `docs/post-onboarding-ux-backlog.md`
  - All listed backlog slices are now implemented in the current branch.
