# Temp Environment Cleanup Failure

## Trigger
- Temp environments remain in `expiring`/`deleted` beyond grace TTL.

## Impact
- Resource leak risk (DB/storage/URL allocations).
- Elevated operational cost.

## Diagnosis
1. List temp environments and filter for stale `expiring`/`deleted` rows.
2. Check `temp_env_expire` job logs for repeated failures.
3. Verify cleanup worker loop and queue depth health.

## Resolution
1. Re-enqueue `temp_env_expire` jobs for stale records.
2. If grace has elapsed, perform manual hard delete of stale temp env rows.
3. Validate no active references remain from workspace/changeset flows.

## Prevention
- Keep TTL scan metrics on dashboard.
- Alert when stale temp env count exceeds a threshold.
- Periodically run manual cleanup drill.
