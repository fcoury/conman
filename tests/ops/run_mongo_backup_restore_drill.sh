#!/usr/bin/env bash
set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
RESULTS_DIR="$ROOT/tests/ops/results"
mkdir -p "$RESULTS_DIR"

TS="$(date +%Y%m%d%H%M%S)"
DB_NAME="conman_backup_drill_${TS}"
COLLECTION="drill_items"
ARCHIVE_PATH="$RESULTS_DIR/${TS}-mongo-backup.archive.gz"
SUMMARY_PATH="$RESULTS_DIR/${TS}-mongo-backup-restore-summary.md"

MONGO_URI_HOST="${MONGO_URI_HOST:-mongodb://127.0.0.1:27019}"
MONGO_CONTAINER="${MONGO_CONTAINER:-conman-mongo-e2e}"
MONGO_URI_CONTAINER="${MONGO_URI_CONTAINER:-mongodb://127.0.0.1:27017}"

if ! command -v mongosh >/dev/null 2>&1; then
  echo "mongosh is required on host" >&2
  exit 1
fi

if ! docker ps --format '{{.Names}}' | rg -x "${MONGO_CONTAINER}" >/dev/null 2>&1; then
  echo "mongo container '${MONGO_CONTAINER}' is not running" >&2
  exit 1
fi

seed_payload='[
  {"k":"alpha","v":11},
  {"k":"beta","v":22},
  {"k":"gamma","v":33}
]'

mongosh "${MONGO_URI_HOST}/${DB_NAME}" --quiet --eval "
db.${COLLECTION}.insertMany(${seed_payload});
print(db.${COLLECTION}.countDocuments({}));
" >/tmp/conman_backup_seed_count.txt

seed_count="$(tail -n1 /tmp/conman_backup_seed_count.txt | tr -d '\r' | xargs)"
rm -f /tmp/conman_backup_seed_count.txt

before_json="$(mongosh "${MONGO_URI_HOST}/${DB_NAME}" --quiet --eval "print(JSON.stringify(db.${COLLECTION}.find({}, {_id:0}).sort({k:1}).toArray()))")"
before_sig="$(printf '%s' "$before_json" | shasum -a 256 | awk '{print $1}')"

docker exec "$MONGO_CONTAINER" sh -lc \
  "mongodump --uri='${MONGO_URI_CONTAINER}/${DB_NAME}' --archive --gzip" > "$ARCHIVE_PATH"

archive_bytes="$(wc -c < "$ARCHIVE_PATH" | tr -d ' ')"

mongosh "${MONGO_URI_HOST}/${DB_NAME}" --quiet --eval "db.dropDatabase();" >/dev/null

cat "$ARCHIVE_PATH" | docker exec -i "$MONGO_CONTAINER" sh -lc \
  "mongorestore --uri='${MONGO_URI_CONTAINER}' --archive --gzip --nsInclude='${DB_NAME}.*'" >/tmp/conman_backup_restore_output.txt

restored_count="$(mongosh "${MONGO_URI_HOST}/${DB_NAME}" --quiet --eval "print(db.${COLLECTION}.countDocuments({}));")"
after_json="$(mongosh "${MONGO_URI_HOST}/${DB_NAME}" --quiet --eval "print(JSON.stringify(db.${COLLECTION}.find({}, {_id:0}).sort({k:1}).toArray()))")"
after_sig="$(printf '%s' "$after_json" | shasum -a 256 | awk '{print $1}')"

if [[ "$seed_count" != "$restored_count" ]]; then
  echo "count mismatch after restore: seeded=${seed_count}, restored=${restored_count}" >&2
  exit 1
fi

if [[ "$before_sig" != "$after_sig" ]]; then
  echo "data signature mismatch after restore" >&2
  exit 1
fi

cat > "$SUMMARY_PATH" <<EOF
# Mongo Backup/Restore Drill (${TS})

- mongo_container: ${MONGO_CONTAINER}
- database: ${DB_NAME}
- collection: ${COLLECTION}
- seeded_count: ${seed_count}
- restored_count: ${restored_count}
- archive_path: tests/ops/results/${TS}-mongo-backup.archive.gz
- archive_bytes: ${archive_bytes}
- data_signature_before: ${before_sig}
- data_signature_after: ${after_sig}
- restore_result: pass

## Notes

- Backup created via \`mongodump --archive --gzip\` inside mongo container.
- Restore executed via \`mongorestore --archive --gzip\` and validated with
  count + sorted payload signature checks.
EOF

rm -f /tmp/conman_backup_restore_output.txt

echo "$SUMMARY_PATH"
