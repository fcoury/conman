# Staged E2E Results

This directory stores staged end-to-end execution artifacts against a live
`gitaly-rs` + gateway setup.

- `run_full_staged_smoke.sh`: full staged API smoke (authoring -> release ->
  deploy/promote/rollback -> temp env lifecycle) against live gitaly-rs,
  including blocked-path and file-size guardrail checks.
- `results/*-create-repo.json`: repository creation responses from gitaly.
- `results/*-full-e2e.log`: captured Conman logs for full staged smoke runs.
- `results/*-full-e2e-summary.md`: summarized outcomes and key IDs from full
  staged smoke runs.
- `results/*-staged-gitaly-attempt.md`: older blocker-focused attempt notes.
- `results/latest-create-repo.json`: latest repository creation payload
  response.
- `results/latest-gitaly-repo.json`: pointer to the latest seeded staged repo.

Execution tracker is maintained in:
- `docs/execution-tracker.md`
