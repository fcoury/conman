# Authentication Failure Spike

## Trigger
- Sudden increase in `conman_auth_failures_total`.

## Impact
- Legitimate users may fail to sign in.
- Potential brute-force activity.

## Diagnosis
1. Break down failure reasons (`unknown_email`, `bad_password`, `invalid_bearer`).
2. Correlate failures by source IP/user agent.
3. Validate JWT secret configuration and token expiry settings.

## Resolution
1. If abuse is suspected, block abusive IP ranges at the edge.
2. Verify auth service config values and restart with corrected settings.
3. Invalidate compromised credentials/tokens if needed.
4. Communicate incident status to app admins.

## Prevention
- Keep rate limits enabled for auth and protected routes.
- Monitor auth-failure anomaly alerts.
- Enforce strong password and reset-token policies.
