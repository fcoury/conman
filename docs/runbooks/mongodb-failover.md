# MongoDB Failover

## Trigger
- `mongo` health check degrades.
- Elevated write/read errors from repository operations.

## Impact
- API mutations can fail.
- Job processing may pause or retry.

## Diagnosis
1. Inspect replica set status and primary election state.
2. Check Conman logs for write concern/selection timeout errors.
3. Validate read/write path with health + lightweight CRUD checks.

## Resolution
1. Restore primary availability (or complete failover election).
2. Ensure application connections recover automatically.
3. Re-run failed jobs where safe.
4. Confirm queue drain resumes.

## Prevention
- Monitor election frequency and replication lag.
- Keep connection timeout settings aligned with failover targets.
- Exercise failover in staging regularly.
