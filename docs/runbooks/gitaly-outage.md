# Gitaly Outage

## Trigger
- Git-backed API operations fail with `git_error`.
- Increased 502/504 rates for workspace and diff endpoints.

## Impact
- Authoring/review flows cannot read or commit repository content.
- Release assembly/publish can stall.

## Diagnosis
1. Check Conman logs for gRPC connection and timeout errors.
2. Verify gitaly-rs process health and network reachability.
3. Confirm storage/disk capacity on the gitaly host.

## Resolution
1. Restart gitaly-rs if process is unhealthy.
2. Restore connectivity between Conman and gitaly endpoint.
3. Re-run blocked git-dependent jobs.
4. Validate with workspace file read/write and release assemble smoke tests.

## Prevention
- Add uptime + latency alerts for gitaly calls.
- Configure retry and timeout budgets per operation.
- Run periodic gitaly failure drills.
