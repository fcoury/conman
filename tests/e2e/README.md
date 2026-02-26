# Staged E2E Results

This directory stores staged end-to-end execution artifacts against a live
`gitaly-rs` + gateway setup.

- `results/*-create-repo.json`: repository creation responses from gitaly.
- `results/*-conman-staged-attempt.log`: captured Conman logs for a staged run.
- `results/*-staged-gitaly-attempt.md`: run summary and blocker details.
- `results/latest-gitaly-repo.json`: pointer to the latest seeded staged repo.

Current blocker summary is tracked in:
- `docs/execution-tracker.md`
