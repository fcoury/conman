#!/usr/bin/env bash
set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
RESULTS_DIR="$ROOT/tests/ops/results"
mkdir -p "$RESULTS_DIR"

TS="$(date +%Y%m%d%H%M%S)"
SUMMARY_PATH="$RESULTS_DIR/${TS}-observability-wiring-summary.md"
COMPOSE_FILE="$ROOT/ops/docker-compose.observability.yml"
KEEP_STACK="${KEEP_STACK:-false}"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required" >&2
  exit 1
fi

cleanup() {
  if [[ "$KEEP_STACK" != "true" ]]; then
    docker compose -f "$COMPOSE_FILE" down >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

docker compose -f "$COMPOSE_FILE" up -d >/dev/null

for _ in {1..40}; do
  if curl -fsS "http://127.0.0.1:9090/-/ready" >/dev/null 2>&1 \
    && curl -fsS "http://127.0.0.1:3001/api/health" >/dev/null 2>&1 \
    && curl -fsS "http://127.0.0.1:9093/-/ready" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

PROM_RULES_JSON="$(curl -fsS "http://127.0.0.1:9090/api/v1/rules")"
ALERTMANAGER_STATUS_JSON="$(curl -fsS "http://127.0.0.1:9093/api/v2/status")"
GRAFANA_HEALTH_JSON="$(curl -fsS "http://127.0.0.1:3001/api/health")"

has_rule() {
  local name="$1"
  printf '%s' "$PROM_RULES_JSON" | jq -e --arg name "$name" \
    '.data.groups[].rules[] | select(.name == $name)' >/dev/null
}

has_rule "ConmanHighApiErrorRate"
has_rule "ConmanJobQueueBacklog"
has_rule "ConmanDeployFailureSpike"
has_rule "ConmanAuthFailureSpike"

if ! printf '%s' "$ALERTMANAGER_STATUS_JSON" | jq -e '.config.original | contains("paging-webhook")' >/dev/null; then
  echo "alertmanager config does not contain paging-webhook route" >&2
  exit 1
fi

dashboard_path="$ROOT/ops/grafana/dashboards/conman-overview.json"
if ! rg -q "conman_http_requests_total" "$dashboard_path"; then
  echo "dashboard missing API metric panel expression" >&2
  exit 1
fi
if ! rg -q "conman_job_queue_depth" "$dashboard_path"; then
  echo "dashboard missing jobs metric panel expression" >&2
  exit 1
fi
if ! rg -q "conman_deployments_total" "$dashboard_path"; then
  echo "dashboard missing deployments metric panel expression" >&2
  exit 1
fi
if ! rg -q "conman_auth_failures_total" "$dashboard_path"; then
  echo "dashboard missing auth-failure metric panel expression" >&2
  exit 1
fi

cat > "$SUMMARY_PATH" <<EOF
# Observability Wiring Check (${TS})

- compose_file: ops/docker-compose.observability.yml
- prometheus_ready: true
- grafana_health: $(printf '%s' "$GRAFANA_HEALTH_JSON" | jq -r '.database')
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
EOF

echo "$SUMMARY_PATH"
