# Release Assembly Failure

## Trigger
- `release_assemble` job fails for a draft release.

## Impact
- The release cannot be published.
- Queued changesets remain blocked from shipment.

## Diagnosis
1. Fetch failing assemble job logs from `GET /api/apps/:appId/jobs/:jobId`.
2. Identify the first conflicting changeset or invalid override in logs.
3. Check release membership/order in `/api/apps/:appId/releases/:releaseId`.

## Resolution
1. Remove the failing changeset from the draft release selection.
2. Move that changeset to `conflicted` or `needs_revalidation` as applicable.
3. Re-run release assemble.
4. Publish once `release_assemble` and `msuite_merge` gates are `succeeded`.

## Prevention
- Keep release batches focused and small.
- Run changeset-level validation before queueing.
- Enforce override collision checks before assembly.
