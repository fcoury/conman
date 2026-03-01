#!/usr/bin/env bash
set -euo pipefail

if ! command -v mkcert >/dev/null 2>&1; then
  echo "mkcert not found. Install it first: https://github.com/FiloSottile/mkcert#installation" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
CERT_DIR="${ROOT_DIR}/ops/local-domains/certs"
DOMAIN_ROOT="${CONMAN_LOCAL_DOMAIN_ROOT:-dxflow-app.localhost}"

mkdir -p "$CERT_DIR"

echo "Installing mkcert local CA ..."
mkcert -install

echo "Generating wildcard certificate for ${DOMAIN_ROOT} ..."
mkcert \
  -cert-file "${CERT_DIR}/${DOMAIN_ROOT}.pem" \
  -key-file "${CERT_DIR}/${DOMAIN_ROOT}.key" \
  "${DOMAIN_ROOT}" "*.${DOMAIN_ROOT}"

echo "Certificate created:"
echo "  ${CERT_DIR}/${DOMAIN_ROOT}.pem"
echo "  ${CERT_DIR}/${DOMAIN_ROOT}.key"
