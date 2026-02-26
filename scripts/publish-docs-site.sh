#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_NAME="${1:-${CLOUDFLARE_PAGES_PROJECT:-conman-docs}}"
BRANCH="${CLOUDFLARE_PAGES_BRANCH:-main}"
DIST_DIR="$ROOT_DIR/docs/site/dist"

echo "Building docs site..."
"$ROOT_DIR/scripts/build-docs-site.sh"

echo "Deploying to Cloudflare Pages..."
echo "  project: $PROJECT_NAME"
echo "  branch:  $BRANCH"
echo "  dir:     $DIST_DIR"

cd "$ROOT_DIR"
wrangler pages deploy "$DIST_DIR" --project-name "$PROJECT_NAME" --branch "$BRANCH"

echo "Publish complete."
