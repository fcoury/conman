# Conman V1 Execution Tracker

Sources:

- [execution-plan.md](./execution-plan.md)
- [IMPLEMENTATION.md](./IMPLEMENTATION.md)
- [conman-v1-backlog.md](./conman-v1-backlog.md)

Use this file as the live control plane for delivery.

## 1) Program Status

- Start date:
- Target date:
- Current wave: `E`
- Current milestone: `M4`
- Overall progress:
  - Epics complete: `12 / 12`
  - Gates passed: `5 / 5`
- Active blockers:
  - none

## 2) Epic Tracker (Dependency Controlled)

Legend:

- Status: `not_started | in_progress | blocked | in_review | done`
- Gate: first wave where epic can merge by dependency

| Epic | Owner | Depends On | Gate | Status | PR/Branch | Checklist % | Blocker |
|---|---|---|---|---|---|---:|---|
| E00 Platform Foundation | worker-platform | none | A | done | master | 100 |  |
| E01 Git Adapter | worker-git | E00 | A | done | master | 100 |  |
| E02 Auth & RBAC | worker-auth | E00 | A | done | master | 100 |  |
| E03 Team/Repo Setup | worker-app | E01, E02 | A | done | master | 100 |  |
| E04 Workspaces | worker-workspace | E01, E03 | B | done | master | 100 |  |
| E05 Changesets | worker-changeset | E02, E04 | B | done | master | 100 |  |
| E06 Async Jobs | worker-jobs | E00, E05 | B | done | master | 100 |  |
| E07 Queue Orchestration | worker-queue-release | E05, E06 | C | done | master | 100 |  |
| E08 Releases | worker-queue-release | E01, E06, E07 | C | done | master | 100 |  |
| E09 Deployments | worker-deploy | E03, E06, E08 | D | done | master | 100 | Deploy gate sequence validated in staged full-flow smoke (`runtime_profile_drift_check` -> `msuite_deploy` -> `deploy_release`) |
| E10 Temp Environments | worker-tempenv | E03, E06 | D | done | master | 100 | Temp env create/provision/extend lifecycle validated in staged full-flow smoke |
| E11 Notifications & Audit | worker-observability | E05-E10 | E | done | master | 100 |  |

## 3) Dependency Gate Rules (Hard Stop)

Do not merge when prerequisites are incomplete:

1. E01/E02 require E00 merged.
2. E03 requires E01 and E02 merged.
3. E04 requires E01 and E03 merged.
4. E05 requires E02 and E04 merged.
5. E06 requires E00 and E05 merged.
6. E07 requires E05 and E06 merged.
7. E08 requires E01, E06, E07 merged.
8. E09 requires E03, E06, E08 merged.
9. E10 requires E03 and E06 merged.
10. E11 requires E05 through E10 merged.

## 4) Wave and Milestone Gates

## Gate A (M1 foundation: E00-E03)

- [x] E00 merged
- [x] E01 merged
- [x] E02 merged
- [x] E03 merged
- [x] Service boots with shared error/pagination/request-id conventions
- [x] Git adapter boundary implemented (no direct gitaly calls in handlers)
- [x] Auth + RBAC enforcement active
- [x] Team/repo/surface + env/runtime-profile baseline APIs available
- Result: `pass`
- Date: `2026-02-26`
- Notes: Full gitaly-rs RPC mappings now implemented in `conman-git::GitalyClient`.

## Gate B (M1 completion: E04-E06)

- [x] E04 merged
- [x] E05 merged
- [x] E06 merged
- [x] End-to-end: author -> submit -> review path works
- [x] Async jobs run and gate transitions
- [x] Required audit events emitted
- Result: `pass`
- Date: `2026-02-26`
- Notes: Live staged runs confirm workspace write + submit/review path with gitaly-rs, including blocked-path and file-size guardrail checks (`tests/e2e/results/20260226044825-full-e2e-summary.md`).

## Gate C (M2: E07-E08)

- [x] E07 merged
- [x] E08 merged
- [x] Queue-first flow works with revalidation
- [x] Config manager can publish subset-based releases
- Result: `pass`
- Date: `2026-02-26`
- Notes: Publish flow now composes selected queued changesets through gitaly merges, fast-forwards integration branch with optimistic checks, and tags releases.

## Gate D (M3: E09-E10)

- [x] E09 merged
- [x] E10 merged
- [x] Deploy/promote/rollback flows pass
- [x] Temp env provisioning + TTL/grace/cleanup pass
- Result: `pass`
- Date: `2026-02-26`
- Notes: Deploy/promote/rollback and deploy-gate jobs reached terminal success in staged run; temp env provision/expire/undo-expire also succeeded (`tests/e2e/results/20260226044825-full-e2e-summary.md`).

## Gate E (M4: E11)

- [x] E11 merged
- [x] Notification coverage complete
- [x] Audit completeness validated
- Result: `pass`
- Date: `2026-02-26`
- Notes: Email notification events and audit completeness remain validated in staged end-to-end runs (`tests/e2e/results/20260226044825-full-e2e-summary.md`).

## 5) CI Quality Gates (Required for Merge)

Every epic PR must pass:

1. Build and test pipeline green.
2. Contract/state-machine tests for touched flows.
3. RBAC enforcement tests for touched endpoints.
4. Audit-event assertions for touched mutations.
5. No dependency rule violations from section 3.

## 6) Weekly Operating Cadence

Twice weekly program review (recommended Tue/Fri):

1. Update epic statuses and checklist %.
2. Review critical path slip risk (E00->E09 chain).
3. Escalate blockers older than 24h.
4. Approve or reject scope changes for active wave.
5. Record next 3 highest-priority tasks per owner.

## 7) Change Control

For any scope change:

- Change summary:
- Affected epics:
- Critical-path impact:
- Approved by:
- Decision date:

Rules:

1. No scope expansion mid-wave unless blocker/severity-high.
2. Any accepted change must update:
   - `conman-v1-scope.md`
   - `conman-v1-backlog.md`
   - affected epic files
3. Recompute milestone gate criteria when dependency changes.

## 8) Implementation Sign-Off

- [x] M1 passed
- [x] M2 passed
- [x] M3 passed
- [x] M4 passed
- [x] No P0 blockers open
- [x] Final sign-off (names/date)

Final sign-off: `2026-02-26` (`fcoury`).
Plan completion gate (strict): latest `tests/ops/results/*-plan-completion-gate-summary.md`.
