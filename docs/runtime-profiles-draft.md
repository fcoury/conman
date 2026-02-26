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
- `app_admin` can perform direct emergency persistent-profile edits (fully
  audited).
- Profile overrides can travel with a changeset/release.
- Changeset submit auto-includes profile overrides and returns a submit summary.
- Validation (`msuite`) runs against both environment and temp profiles, with
  configurable gates and command.
- Deployment is blocked on runtime profile drift until revalidation passes.
- Database engine scope for v1 is MongoDB only.
- Special base databases/profiles are managed by `app_admin` only.
- If two queued changesets override the same env var key, the later one becomes
  `conflicted`.
- If both queued changesets set the same env var key to the same typed value,
  it is not treated as a conflict.
- Canonical env profile approval policy is configurable: `same_as_changeset` or
  `stricter_two_approvals` (default).
- Drift includes env vars, secrets, URL, DB settings, and migration set changes.
- Secret key rotation is manual in v1.
- Secrets can be revealed in plaintext by `app_admin` only; other roles see
  masked previews.
- Secret reveal does not require re-auth/audit reason in v1.
- MongoDB clone strategy defaults to snapshot clone, fallback to dump/restore.
- Temp URL pattern omits `conman` branding in hostname.
- Env vars are typed in v1: `string | number | boolean | json`.
- Runtime profile schema is strict typed in v1 (no arbitrary custom top-level
  fields).
- Changeset profile overrides are stored in a separate
  `changeset_profile_overrides` collection.
- Applied migration metadata is tracked in Conman DB for drift/revalidation.
- Temp profile base selection priority is app default base profile first, then
  user override at temp-env creation.
- Temp URLs are unique per temp-env instance (not workspace-stable).
- Deploy drift remediation should offer "create drift-fix changeset".
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
- `env_vars` (typed: `string | number | boolean | json`)
- `secrets` (encrypted at rest; optional external secret refs later)
- `database`:
  - `engine` (`mongodb`)
  - `connection_ref`
  - `provisioning_mode` (`existing | clone_from_base | snapshot_restore | empty`)
  - `base_profile_id?`
- `migrations`:
  - `repo_paths[]`
  - `command_ref`
  - `applied_state_ref`
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
8. Track applied migration metadata in Conman for drift and gating decisions.
9. Support app-admin direct profile edits with audit and drift-triggered
   revalidation requirements.
10. Support secret masking/reveal policy (`app_admin` plaintext only).

## 6) Non-Goals (Initial Draft)

- Full external secret manager integration in v1.
- Arbitrary cross-app profile sharing.
- Runtime profile templating language.
- Arbitrary custom runtime-profile top-level schema fields.
- Multiple baseline dataset variants (single baseline path in v1).

## 7) Resolved v1 Policy

1. Secret encryption model:
   - Envelope encryption in-app.
   - Service master key in config, manual key rollover in v1.
2. Database clone strategy (MongoDB):
   - Snapshot clone by default, dump/restore fallback.
3. URL generation:
   - Host pattern uses readable short IDs:
     `{app}-{kind}-{word}.<domain>`.
4. Changeset-coupled profile overrides:
   - Stored in `changeset_profile_overrides`.
   - Auto-included on submit with submit summary.
5. Validation gates:
   - submit: temp profile only.
   - release publish: environment profiles only.
   - deploy: target environment profile only.
6. Secret visibility:
   - `app_admin` can reveal plaintext.
   - Other roles see masked values.
   - Masking policy:
     - length <= 8: show last 4 characters only.
     - length > 8: show first 4 and last 4.
7. Drift remediation:
   - Deployment remains blocked until revalidation passes.
   - UX offers creation of a drift-fix changeset.

## 8) Implementation Direction (v1)

- Secrets encryption: use envelope encryption in-app with Rust crates
  `aes-gcm`, `rand`, and `zeroize` (no external secret manager dependency).
- URL short IDs: use human-readable word IDs (for example from a wordlist or a
  petname-style generator) instead of opaque UUID-only hostnames.
- If opaque UUIDs are used anywhere in runtime-profile tooling/metadata, use
  UUIDv7.
- Use a typed env var value enum and validate per-key type at write-time.
- Persist applied migration metadata (`migration_executions`) keyed by app/env/
  runtime profile revision.
