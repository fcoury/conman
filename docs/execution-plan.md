# Conman V1 Multi-Agent Execution Plan

Source: [IMPLEMENTATION.md](./IMPLEMENTATION.md) (`Epic Index`, `Critical path`,
`Milestone mapping`)

## 1) Objective

Execute Conman v1 with parallel sub-agents while preserving dependency order on
the critical path:

`E00 -> E01 -> E03 -> E04 -> E05 -> E06 -> E07 -> E08 -> E09`

Parallelizable after E06:

- E08 can proceed with E10
- E11 can run alongside E09/E10

## 2) Agent Roles

1. **Orchestrator (primary)**: dependency control, integration decisions,
   cross-epic contract ownership, merge sequencing.
2. **Explorer agents**: fast, targeted reading of epic specs and current code
   before each wave starts.
3. **Worker agents**: implementation owners per epic/file surface.
4. **Awaiter agent**: all long-running builds/tests/integration checks.

## 3) Epic Ownership

1. `worker-platform`: E00 Platform Foundation
2. `worker-git`: E01 Git Adapter
3. `worker-auth`: E02 Auth & RBAC
4. `worker-app`: E03 Tenant/Repo Setup (tenants/repos/surfaces + settings/env/runtime profiles)
5. `worker-workspace`: E04 Workspaces
6. `worker-changeset`: E05 Changesets
7. `worker-jobs`: E06 Async Jobs + gates
8. `worker-queue-release`: E07 Queue + E08 Releases
9. `worker-deploy`: E09 Deployments
10. `worker-tempenv`: E10 Temp Environments
11. `worker-observability`: E11 Notifications/Audit

## 4) Wave Plan

## Wave A (M1 foundation)

- Implement: E00
- Parallel after E00: E01 + E02
- Then: E03

Gate A exit:

- Service boots with shared conventions (error envelope, pagination,
  request-id UUIDv7).
- Git adapter boundary in place.
- Auth/RBAC active.
- Tenant/repo/surface + environment/runtime-profile baseline APIs available.

## Wave B (M1 completion)

- Implement: E04 -> E05 -> E06

Gate B exit:

- End-to-end authoring and review works.
- Async jobs execute and enforce submit/release/deploy gates.

## Wave C (M2)

- Implement: E07 -> E08

Gate C exit:

- Queue-first flow operational.
- Config manager can assemble/publish subset-based releases.

## Wave D (M3)

- Implement in parallel where valid:
  - E10 after E06
  - E09 after E08 (and E03/E06 prerequisites)

Gate D exit:

- Deploy/promote/rollback paths complete.
- Temp environments provision/expire/cleanup with TTL+grace behavior.

## Wave E (M4)

- Implement: E11

Gate E exit:

- Notification and audit coverage complete.
- Event fanout and user preference behavior validated.

## 5) Coordination Rules

1. Freeze shared contracts at each gate:
   - `conman-core` domain/state machine contracts
   - repository traits
   - API response/error envelope
2. Keep single-owner file surfaces per epic to minimize merge conflicts.
3. Integrate by wave checkpoints, not only at milestone end.
4. Run long integration checks via awaiter agent at each gate.
5. Track and resolve cross-epic blockers immediately (especially E03/E06/E08
   boundaries).

## 6) Per-Wave Validation Checklist

1. Compile/build green.
2. Contract tests for shared APIs/domain transitions green.
3. Role enforcement tests green.
4. Async job state machine and retry/timeout behavior validated.
5. Audit events emitted for all required mutations in the wave.
6. Documentation updated for any contract/state changes.

## 7) Milestone Mapping

1. **M1: Authoring + Review**: E00-E06
2. **M2: Queue + Release**: E07-E08
3. **M3: Environments + Recovery**: E09-E10
4. **M4: Notifications + Audit**: E11
