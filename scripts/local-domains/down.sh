#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
OPS_DIR="${ROOT_DIR}/ops/local-domains"

docker compose -f "${OPS_DIR}/docker-compose.yml" down
