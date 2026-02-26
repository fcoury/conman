# Go-Live Checklist

## Platform
- [ ] MongoDB backups and restore drill verified.
- [x] gitaly-rs health and restart procedure verified.
- [ ] Secrets master key configured and rotation runbook documented.

## Product Flows
- [x] Workspace authoring and changeset review smoke-tested.
- [x] Queue-first release publish smoke-tested.
- [ ] Deploy/promote/rollback smoke-tested.
- [ ] Temp env create/expire/undo-expire smoke-tested.

## Operations
- [ ] Dashboards include API, jobs, deployments, and auth-failure metrics.
- [ ] Alert routing verified for paging channels.
- [ ] All runbooks in `docs/runbooks/` reviewed by on-call owner.

## Security
- [x] Password, token expiry, and RBAC policies verified.
- [ ] Blocked-path and file-size guardrails verified.
- [x] Audit trails verified for privileged actions.
