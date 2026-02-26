# Conman Runtime Profiles (Draft)

## 1) Problem Statement

Conman currently models deployment environments and temporary databases, but the
real operational unit is broader than a database:

- base URL and routing context
- environment variables / secrets
- database connection and provisioning behavior
- seed/baseline data strategy
- optional service endpoints and toggles

This draft introduces a single abstraction to represent that unit
consistently for permanent environments and temporary workspace/changeset
environments.

## 2) Proposed Abstraction

### Runtime Profile

A **Runtime Profile** is the full runtime configuration required to execute and
validate an app build/release.

Suggested shape:

- `name`
- `kind`: `persistent_env | temp_workspace | temp_changeset`
- `base_url`
- `env_vars` (non-secret)
- `secret_refs` (references only, not secret values)
- `database`:
  - `engine`
  - `connection_ref`
  - `provisioning_mode` (`existing | clone_from_base | snapshot_restore | empty`)
  - `base_profile_id?`
- `data_strategy`:
  - `seed_mode` (`none | baseline | fixture_set | snapshot`)
  - `seed_source_ref?`
- `lifecycle`:
  - `ttl_idle_hours?`
  - `grace_hours?`
  - `auto_cleanup`

## 3) Mapping to Existing Model

- Existing `Environment` keeps pipeline order, approvals, and promotion logic.
- New `RuntimeProfile` is attached to each environment (`environment.profile_id`).
- Temp env creation produces a temp runtime profile derived from either:
  - workspace + selected base persistent profile, or
  - changeset + selected base persistent profile.

## 4) Baseline for Workspace Temp DBs

Candidate default for v1:

- Temp workspace profile clones from the `Development` environment runtime
  profile by default (configurable per app).
- Override allowed at creation time for config managers/app admins.

Rationale:

- closer to day-to-day iteration data
- avoids production-sensitive data coupling
- predictable for msuite and reviewer flows

## 5) Suggested V1 Scope

1. Add runtime profile storage and references.
2. Allow per-environment env vars + URL + DB config.
3. Temp env provisioning chooses a base profile and clone strategy.
4. Persist profile revisions (append-only) for auditability.
5. Keep secrets as references (integrate secret backend later).

## 6) Non-Goals (Initial Draft)

- Full secret manager integration in v1 (store references only).
- Arbitrary cross-app profile sharing.
- Runtime profile templating language.

## 7) Open Design Questions

See active scoping questions in conversation; these must be resolved before
locking implementation details.
