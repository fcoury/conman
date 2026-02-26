# Observability Rollout

## Production Wiring Artifacts
- Alerts: `ops/alerts/conman-alerts.yml`
- Alertmanager routing: `ops/alertmanager/alertmanager.yml`
- Prometheus config: `ops/prometheus/prometheus.yml`
- Grafana dashboard JSON: `ops/grafana/dashboards/conman-overview.json`
- Grafana provisioning:
  - `ops/grafana/provisioning/datasources/prometheus.yml`
  - `ops/grafana/provisioning/dashboards/dashboards.yml`
- Local stack compose file: `ops/docker-compose.observability.yml`

## Local Verification
From repository root:

```bash
cd ops
docker compose -f docker-compose.observability.yml up -d
```

Then verify:
- Prometheus: `http://localhost:9090`
- Grafana: `http://localhost:3001` (admin/admin)
- Alertmanager: `http://localhost:9093`

## Production Checklist
1. Mount `conman-alerts.yml` into Prometheus rules path.
2. Mount Grafana provisioning and dashboard directories.
3. Point scrape target in Prometheus config to Conman production endpoint.
4. Import/update dashboard and validate all panel queries.
5. Validate alert routes for queue depth, deploy failures, auth spikes, and 5xx.

## Drill Evidence
- Load drill: `tests/load/results/2026-02-26-report.md`
- Fault drill: `tests/fault/results/2026-02-26-report.md`
- Observability wiring check: `tests/ops/results/20260226044706-observability-wiring-summary.md`
