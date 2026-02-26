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
  - Epics complete: `10 / 13`
  - Gates passed: `2 / 5`
- Active blockers:
  - E12 still needs staged real-repo load/fault runs (with test gitaly + seeded repos)

## 2) Epic Tracker (Dependency Controlled)

Legend:

- Status: `not_started | in_progress | blocked | in_review | done`
- Gate: first wave where epic can merge by dependency

| Epic | Owner | Depends On | Gate | Status | PR/Branch | Checklist % | Blocker |
|---|---|---|---|---|---|---:|---|
| E00 Platform Foundation | worker-platform | none | A | done | master | 100 |  |
| E01 Git Adapter | worker-git | E00 | A | done | master | 100 |  |
| E02 Auth & RBAC | worker-auth | E00 | A | done | master | 100 |  |
| E03 App Setup | worker-app | E01, E02 | A | done | master | 100 |  |
| E04 Workspaces | worker-workspace | E01, E03 | B | done | master | 100 |  |
| E05 Changesets | worker-changeset | E02, E04 | B | done | master | 100 |  |
| E06 Async Jobs | worker-jobs | E00, E05 | B | done | master | 100 |  |
| E07 Queue Orchestration | worker-queue-release | E05, E06 | C | done | master | 100 |  |
| E08 Releases | worker-queue-release | E01, E06, E07 | C | done | master | 100 |  |
| E09 Deployments | worker-deploy | E03, E06, E08 | D | in_progress | master | 92 | Drift/deploy gates are command-backed; provider-specific deployment executors remain app-specific |
| E10 Temp Environments | worker-tempenv | E03, E06 | D | in_progress | master | 95 | Provision/expire hooks are command-backed and TTL/grace cleanup is active; provider integrations still app-specific |
| E11 Notifications & Audit | worker-observability | E05-E10 | E | done | master | 100 |  |
| E12 Hardening | worker-observability | E08-E11 | E | in_progress | master | 88 | Local load/fault drills and dashboard provisioning wiring are done; staged real-repo load/fault runs remain |

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
11. E12 requires E08 through E11 merged.

## 4) Wave and Milestone Gates

## Gate A (M1 foundation: E00-E03)

- [x] E00 merged
- [x] E01 merged
- [x] E02 merged
- [x] E03 merged
- [x] Service boots with shared error/pagination/request-id conventions
- [x] Git adapter boundary implemented (no direct gitaly calls in handlers)
- [x] Auth + RBAC enforcement active
- [x] App/env/runtime-profile baseline APIs available
- Result: `pass`
- Date: `2026-02-26`
- Notes: Full gitaly-rs RPC mappings now implemented in `conman-git::GitalyClient`.

## Gate B (M1 completion: E04-E06)

- [x] E04 merged
- [x] E05 merged
- [x] E06 merged
- [ ] End-to-end: author -> submit -> review path works
- [x] Async jobs run and gate transitions
- [x] Required audit events emitted
- Result: `pass | fail`
- Date:
- Notes:

## Gate C (M2: E07-E08)

- [x] E07 merged
- [x] E08 merged
- [x] Queue-first flow works with revalidation
- [x] Config manager can publish subset-based releases
- Result: `pass`
- Date: `2026-02-26`
- Notes: Publish flow now composes selected queued changesets through gitaly merges, fast-forwards integration branch with optimistic checks, and tags releases.

## Gate D (M3: E09-E10)

- [ ] E09 merged
- [ ] E10 merged
- [ ] Deploy/promote/rollback flows pass
- [ ] Temp env provisioning + TTL/grace/cleanup pass
- Result: `pass | fail`
- Date:
- Notes:

## Gate E (M4: E11-E12)

- [x] E11 merged
- [ ] E12 merged
- [ ] Notification coverage complete
- [x] Audit completeness validated
- [ ] Hardening/runbooks/load/fault checks complete
- Result: `pass | fail`
- Date:
- Notes: Local load/fault drill artifacts and production dashboard provisioning files were added; staged real-repo drills remain open.

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

## 8) Launch Readiness Sign-Off

- [ ] M1 passed
- [ ] M2 passed
- [ ] M3 passed
- [ ] M4 passed
- [ ] No P0 blockers open
- [ ] Runbooks approved
- [ ] Observability dashboards active
- [ ] Security checklist complete
- [ ] Final sign-off (names/date)
