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
  - Epics complete: `6 / 13`
  - Gates passed: `0 / 5`
- Active blockers:
  - E01 still needs concrete gitaly-rs RPC mappings beyond adapter stubs
  - E12 still needs executed load/fault results and production dashboard wiring

## 2) Epic Tracker (Dependency Controlled)

Legend:

- Status: `not_started | in_progress | blocked | in_review | done`
- Gate: first wave where epic can merge by dependency

| Epic | Owner | Depends On | Gate | Status | PR/Branch | Checklist % | Blocker |
|---|---|---|---|---|---|---:|---|
| E00 Platform Foundation | worker-platform | none | A | done | master | 100 |  |
| E01 Git Adapter | worker-git | E00 | A | in_review | master | 80 | Full gitaly-rs RPC mappings pending |
| E02 Auth & RBAC | worker-auth | E00 | A | done | master | 100 |  |
| E03 App Setup | worker-app | E01, E02 | A | done | master | 100 |  |
| E04 Workspaces | worker-workspace | E01, E03 | B | done | master | 100 |  |
| E05 Changesets | worker-changeset | E02, E04 | B | done | master | 100 |  |
| E06 Async Jobs | worker-jobs | E00, E05 | B | done | master | 100 |  |
| E07 Queue Orchestration | worker-queue-release | E05, E06 | C | in_progress | master | 85 | Revalidation uses queue conflict + simulated gate flow; external msuite execution still noop |
| E08 Releases | worker-queue-release | E01, E06, E07 | C | in_progress | master | 85 | Git composition/publish must be tied to gitaly operations |
| E09 Deployments | worker-deploy | E03, E06, E08 | D | in_progress | master | 85 | Drift checks and deploy approvals are enforced; real execution hooks are still noop |
| E10 Temp Environments | worker-tempenv | E03, E06 | D | in_progress | master | 90 | Runtime cleanup worker is active; provider-side teardown hooks remain stubbed |
| E11 Notifications & Audit | worker-observability | E05-E10 | E | in_progress | master | 90 | Outbox drain + SMTP provider path shipped; full audit completeness assertions pending |
| E12 Hardening | worker-observability | E08-E11 | E | in_progress | master | 72 | Metrics, throttling, runbooks, test scaffolding, security guards, and alert/dashboard artifacts shipped; load/fault execution pending |

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
- [ ] E01 merged
- [x] E02 merged
- [x] E03 merged
- [x] Service boots with shared error/pagination/request-id conventions
- [ ] Git adapter boundary implemented (no direct gitaly calls in handlers)
- [x] Auth + RBAC enforcement active
- [x] App/env/runtime-profile baseline APIs available
- Result: `pass | fail`
- Date:
- Notes:

## Gate B (M1 completion: E04-E06)

- [x] E04 merged
- [x] E05 merged
- [x] E06 merged
- [ ] End-to-end: author -> submit -> review path works
- [x] Async jobs run and gate transitions
- [ ] Required audit events emitted
- Result: `pass | fail`
- Date:
- Notes:

## Gate C (M2: E07-E08)

- [ ] E07 merged
- [ ] E08 merged
- [ ] Queue-first flow works with revalidation
- [ ] Config manager can publish subset-based releases
- Result: `pass | fail`
- Date:
- Notes:

## Gate D (M3: E09-E10)

- [ ] E09 merged
- [ ] E10 merged
- [ ] Deploy/promote/rollback flows pass
- [ ] Temp env provisioning + TTL/grace/cleanup pass
- Result: `pass | fail`
- Date:
- Notes:

## Gate E (M4: E11-E12)

- [ ] E11 merged
- [ ] E12 merged
- [ ] Notification coverage complete
- [ ] Audit completeness validated
- [ ] Hardening/runbooks/load/fault checks complete
- Result: `pass | fail`
- Date:
- Notes:

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
