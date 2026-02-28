# Conman Web Interface Plan (Single-Repo Instance, Brand-New UI)

## Summary
Build a new React + Vite + TypeScript web app in `web/`, styled with a Gistia-inspired Tailwind v4 + shadcn/Radix approach, focused on one bound repo per UI instance.
No API-breaking changes are required for this plan.
We will add only additive backend endpoints and routes needed to support setup, binding, invites listing, and backend-served SPA mode.

## Public API / Interface Changes
1. Add `GET /api/repo`.
- Purpose: frontend bootstrap for single-repo instance mode.
- Auth: required.
- Response envelope: `ApiResponse`.
- `data` shape:
  - `status`: `"bound"` or `"unbound"`.
  - `binding`: binding metadata or `null`.
  - `repo`: repo object or `null`.
  - `team`: team object or `null`.
  - `apps`: app list for bound repo.
  - `role`: current user role on bound repo or `null`.
  - `can_rebind`: boolean.
- Behavior:
  - Returns `"unbound"` when instance has no binding.
  - Returns `403` when instance is bound but user has no access (UI shows access-denied screen).

2. Add `PATCH /api/repo`.
- Purpose: admin rebind of instance to a different repo.
- Auth: required, `admin`/`owner` on target repo.
- Request body: `{ "repo_id": "<id>" }`.
- Response: same payload shape as `GET /api/repo`.

3. Add `GET /api/teams/{teamId}/invites`.
- Purpose: pending invite visibility for members/invites UI.
- Auth: team admin.
- Response: paginated `ApiResponse<Vec<Invite>>` with existing `page/limit` conventions.

4. Add backend SPA serving routes.
- Serve frontend from `/` (and non-API paths) for static assets + history fallback.
- Keep `/api/*`, `/api/docs`, `/api/openapi.json` unchanged and isolated from SPA fallback.

5. Keep existing signup contract unchanged.
- `POST /api/auth/signup` continues auto-bootstrapping first team + repo.
- Setup wizard will use resulting resources and then bind instance context.

## Backend Data/Type Additions
1. Add a singleton UI binding store in MongoDB (new collection, no migration script).
- Collection example: `ui_config`.
- Singleton document keyed by `_id: "default"`.
- Fields: `repo_id`, `configured_by`, `configured_at`, `updated_at`.
- Team and apps are derived from bound repo at read time.

2. Add corresponding domain/repo layers.
- `conman-core`: lightweight UI binding type(s).
- `conman-db`: `UiConfigRepo` with `get`, `set`, `ensure_indexes`.
- `conman-api`: handlers + route wiring + auth/role checks + audit events for binding changes.

## Frontend Build and Runtime Architecture
1. Create `web/` app with `pnpm`.
- Stack: React 19, React Router, TanStack Query, Zod-based API parsing, Playwright, Vitest.
- Styling: Tailwind v4 + shadcn/Radix primitives, Gistia-like token model.
- API calls: always relative `/api/*`.
- Dev mode: Vite proxy forwards `/api` to backend.

2. Route model.
- Public routes: `/login`, `/signup`, `/forgot-password`, `/reset-password`, `/accept-invite`.
- Protected routes: `/setup`, `/workspaces`, `/changesets`, `/releases`, `/deployments`, `/runtime`, `/temp-envs`, `/jobs`, `/apps`, `/members`, `/notifications`, `/settings`.
- Boot flow:
  - After auth, call `GET /api/repo`.
  - `status=unbound` => go to setup wizard.
  - `403` => access denied screen.
  - `status=bound` => open console.

3. Setup wizard scope.
- Step sequence: authenticate, team/repo setup, app creation, bind repo.
- Uses existing team/repo/app endpoints plus new `PATCH /api/repo`.

## Module Scope (First Delivery)
1. Auth lifecycle UI: signup, login, logout, forgot/reset password, invite acceptance.
2. Setup/admin: create team/repo/app, bind/rebind repo.
3. Apps: list/create/update repo apps.
4. Workspaces: list/create/get/update/reset/sync.
5. File editing:
- Monaco editor with guardrails.
- Text/code files editable.
- Large files: read-only preview + download.
- Tree path filter included.
- Base64 decode/encode handled in API client layer.
- Syntax/format guardrails for YAML/JSON saves.
6. Changesets: list/create/update/get/detail, submit/resubmit, review, queue, move-to-draft, diff view, comments.
7. Releases: list/create/get, set changesets, reorder, assemble, publish.
8. Deployments: deploy/promote/rollback/list with happy-path + key guardrails.
9. Runtime/environments: list/create/update runtime profiles, environment replacement, secret masked display + admin reveal.
10. Temp envs: list/create/extend/undo-expire/delete.
11. Jobs: list/detail with auto-polling for active jobs.
12. Members/invites: member list/assignment and invite list/create/resend/revoke.
13. Notification preferences: get/update.

## RBAC and UX Rules
1. Client behavior: hide restricted actions (not disabled-with-reason).
2. Server remains source of truth; UI still handles and displays API authorization errors.
3. Repo instance model: one bound repo per UI instance/session context.

## Testing Plan
1. Frontend unit tests (Vitest + RTL).
- API client parsing/envelope handling.
- Auth state transitions.
- Route guards.
- Editor guardrail logic.
- Permission-based rendering rules.

2. Frontend integration tests (MSW).
- Auth flows.
- Setup binding flow.
- Workspace file fetch/save/delete.
- Changeset submit/review/queue job transitions.
- Runtime secret reveal permission handling.

3. E2E smoke tests (Playwright).
- Signup/login -> setup wizard -> bind -> land on workspaces.
- Workspace file edit/save/checkpoint on YAML file.
- Changeset happy path to release publish.
- Deployment + job polling completion.
- Admin rebind and access-denied behavior for unauthorized user.

## Acceptance Criteria
1. User can fully operate scoped workflows through the UI without using curl/manual API scripts.
2. Single-repo binding is enforced via `GET /api/repo` and configurable via admin rebind UI.
3. Setup wizard works for unbound instances and persists binding.
4. Invites can be listed and managed in UI.
5. Backend can serve frontend at `/` while separate Vite dev mode also works.
6. No API-breaking changes are introduced.

## Assumptions and Defaults
1. No migrations/backward-compat work is required in this phase.
2. No ops gate/documentation formalization work is included now.
3. Additive API changes are acceptable and preferred over breaking contracts.
4. UI visual direction mirrors Gistia-style primitives/tokens but remains self-contained inside this repo.
5. Editor target is YAML/JS/TS-heavy config repos, not React source editing.
