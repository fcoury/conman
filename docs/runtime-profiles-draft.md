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

## 1.1) Decisions Captured

- Runtime profiles are versioned with releases.
- Variable precedence is `app defaults < environment profile < temp overrides`.
- Temp profiles are derived from a base persistent environment profile.
- Temp environment URLs are auto-generated and should be readable/shareable.
- Environment profile changes for canonical user-facing env require approval.
- Profile overrides can travel with a changeset/release.
- Validation (`msuite`) runs against both environment and temp profiles, with
  configurable gates and command.
- Deployment is blocked on runtime profile drift until revalidation passes.
- Database engine scope for v1 is MongoDB only.
- Special base databases/profiles are managed by `app_admin` only.
- If two queued changesets override the same env var key, the later one becomes
  `conflicted`.
- Canonical env profile approval policy is configurable: `same_as_changeset` or
  `stricter_two_approvals` (default).
- Drift includes env vars, secrets, URL, DB settings, and migration set changes.
- Secret key rotation is manual in v1.
- MongoDB clone strategy defaults to snapshot clone, fallback to dump/restore.
- Temp URL pattern omits `conman` branding in hostname.
- Changeset profile overrides are stored in a separate
  `changeset_profile_overrides` collection.
- Validation defaults:
  - submit: temp profile only
  - release publish: environment profiles only
  - deploy: target environment profile only

## 2) Proposed Abstraction

### Runtime Profile

A **Runtime Profile** is the full runtime configuration required to execute and
validate an app build/release.

Suggested shape:

- `name`
- `kind`: `persistent_env | temp_workspace | temp_changeset`
- `base_url`
- `env_vars` (non-secret)
- `secrets` (encrypted at rest; optional external secret refs later)
- `database`:
  - `engine` (`mongodb`)
  - `connection_ref`
  - `provisioning_mode` (`existing | clone_from_base | snapshot_restore | empty`)
  - `base_profile_id?`
- `data_strategy`:
  - `seed_mode` (`none | baseline | snapshot`)
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
- A special app-defined base profile is also supported (for curated test data).
- Override allowed at creation time.

Rationale:

- closer to day-to-day iteration data
- avoids production-sensitive data coupling
- predictable for msuite and reviewer flows

## 5) Suggested V1 Scope

1. Add runtime profile storage and references.
2. Allow per-environment env vars + URL + DB config.
3. Temp env provisioning chooses a base profile and clone strategy.
4. Persist profile revisions (append-only) for auditability.
5. Encrypt secrets at rest in Conman (no external dependency required for v1).
6. Support changeset-bound profile overrides that can be released/promoted.
7. Enforce override-key conflict detection during queue/release composition.

## 6) Non-Goals (Initial Draft)

- Full external secret manager integration in v1.
- Arbitrary cross-app profile sharing.
- Runtime profile templating language.
- Multiple baseline dataset variants (single baseline path in v1).

## 7) Open Design Questions

1. Secret encryption model:
   - app-level data encryption key (DEK) envelope-encrypted by a service master
     key from env/config?
   - key rollover is manual in v1.
2. Database clone strategy (MongoDB):
   - snapshot clone by default, fallback to dump/restore.
3. URL generation:
   - host pattern with readable short ID (recommended:
     `{app}-{kind}-{word}.<domain>`).
4. Changeset-coupled profile overrides:
   - stored in `changeset_profile_overrides` collection.
5. Validation gates:
   - submit: temp profile only.
   - release publish: environment profiles only.
   - deploy: target environment profile only.

## 8) Implementation Direction (v1)

- Secrets encryption: use envelope encryption in-app with Rust crates
  `aes-gcm`, `rand`, and `zeroize` (no external secret manager dependency).
- URL short IDs: use human-readable word IDs (for example from a wordlist or a
  petname-style generator) instead of opaque UUID-only hostnames.
