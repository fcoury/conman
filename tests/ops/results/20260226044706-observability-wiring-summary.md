# Observability Wiring Check (20260226044706)

- compose_file: ops/docker-compose.observability.yml
- prometheus_ready: true
- grafana_health: ok
- alertmanager_ready: true
- rules_verified:
  - ConmanHighApiErrorRate
  - ConmanJobQueueBacklog
  - ConmanDeployFailureSpike
  - ConmanAuthFailureSpike
- alert_routing_verified: paging-webhook route present in alertmanager config
- dashboard_metrics_verified:
  - conman_http_requests_total
  - conman_job_queue_depth
  - conman_deployments_total
  - conman_auth_failures_total
- result: pass
