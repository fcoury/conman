# Load Test Scripts

This directory contains k6 scenarios for Conman hot paths. Run with:

```bash
k6 run tests/load/concurrent_edits.js
```

Expected environment variables:
- `BASE_URL`
- `APP_ID`
- `TOKEN`

Latest recorded run:
- `tests/load/results/2026-02-26-report.md`
