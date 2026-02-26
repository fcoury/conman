# Mongo Backup/Restore Drill (20260226044657)

- mongo_container: conman-mongo-e2e
- database: conman_backup_drill_20260226044657
- collection: drill_items
- seeded_count: 3
- restored_count: 3
- archive_path: tests/ops/results/20260226044657-mongo-backup.archive.gz
- archive_bytes: 411
- data_signature_before: b68e2e6ec7a2358050491fb19bb78e387f577965f45096fa22f6e67e21c3da25
- data_signature_after: b68e2e6ec7a2358050491fb19bb78e387f577965f45096fa22f6e67e21c3da25
- restore_result: pass

## Notes

- Backup created via `mongodump --archive --gzip` inside mongo container.
- Restore executed via `mongorestore --archive --gzip` and validated with
  count + sorted payload signature checks.
