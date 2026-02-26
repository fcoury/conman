# Load Test Scripts

This directory contains k6 scenarios for Conman hot paths. Run with:

```bash
k6 run tests/load/concurrent_edits.js
```

Expected environment variables:
- `BASE_URL`
- `APP_ID`
- `TOKEN`
