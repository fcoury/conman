# Deployment Failure

## Trigger
- Deployment state ends in `failed`.
- `deploy_release` or `msuite_deploy` job fails.

## Impact
- Target environment is not updated.
- Promotion sequence is blocked.

## Diagnosis
1. Fetch deployment and related job logs.
2. Determine whether failure is gate validation, runtime drift, or execution.
3. Confirm release/tag and environment mapping are correct.

## Resolution
1. Fix failing gate or drift issue.
2. Re-run deployment if no rollback is needed.
3. If necessary, execute rollback:
   - mode A: revert integration + new release
   - mode B: redeploy prior tag

## Prevention
- Keep drift checks green before deploy attempts.
- Avoid concurrent exceptional deploys unless required.
- Use staged promotions for risky changes.
