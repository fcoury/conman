# Secrets Master Key Rotation

## Scope

Rotate `CONMAN_SECRETS_MASTER_KEY` used by Conman to encrypt persisted runtime
profile secrets.

## Preconditions

1. Planned maintenance window with config-manager stakeholders.
2. Verified fresh MongoDB backup (see `tests/ops/run_mongo_backup_restore_drill.sh`).
3. New key generated and distributed through your secret manager.

## Procedure (v1)

1. Stop Conman API/job-runner writes to runtime profile secrets.
2. Export all runtime profiles and encrypted secret payloads from MongoDB.
3. Run one-off re-encryption migration:
   - decrypt each secret with old key
   - re-encrypt with new key
   - update `runtime_profiles.secrets_encrypted` atomically
4. Deploy Conman with new `CONMAN_SECRETS_MASTER_KEY`.
5. Run smoke checks:
   - runtime profile list/get works
   - secret reveal endpoint works for `app_admin`
   - deploy drift gate jobs succeed for at least one app/env

## Rollback

1. Revert to previous Conman deployment and old key.
2. Restore runtime profile documents from backup if needed.
3. Re-run smoke checks to confirm decryption path is healthy.

## Evidence to Capture

- Change request/ticket ID.
- Backup artifact reference.
- Migration execution logs.
- Post-rotation smoke result artifact paths.
