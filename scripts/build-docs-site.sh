#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$ROOT_DIR/docs"
SITE_DIR="$ROOT_DIR/docs/site"
DIST_DIR="$SITE_DIR/dist"

mkdir -p "$DIST_DIR"

cp "$SITE_DIR/style.css" "$DIST_DIR/style.css"
# Copy logo with cropped viewBox (original is 500x500 but content ends at ~385)
sed 's/viewBox="0.00 0.00 500.00 500.00"/viewBox="0 0 385 385"/' \
  "$ROOT_DIR/assets/conman.svg" > "$DIST_DIR/conman.svg"

# Common Pandoc flags for doc pages
PANDOC_OPTS=(
  --from=gfm
  --to=html5
  --standalone
  --toc
  --toc-depth=3
  --metadata toc-title="Contents"
  --template="$SITE_DIR/template.html"
  --syntax-highlighting=breezedark
)

build_page() {
  local source="$1"
  local output="$2"
  local title="$3"
  local css_path="$4"
  shift 4
  pandoc "$source" \
    "${PANDOC_OPTS[@]}" \
    --css "$css_path" \
    --metadata title="$title" \
    "$@" \
    -o "$output"
}

build_page \
  "$SRC_DIR/conman-v1-scope.md" \
  "$DIST_DIR/conman-v1-scope.html" \
  "Conman V1 Scope" \
  "style.css"

build_page \
  "$SRC_DIR/conman-v1-backlog.md" \
  "$DIST_DIR/conman-v1-backlog.html" \
  "Conman V1 Backlog" \
  "style.css"

build_page \
  "$SRC_DIR/IMPLEMENTATION.md" \
  "$DIST_DIR/implementation.html" \
  "V1 Implementation Guide" \
  "style.css"

build_page \
  "$SRC_DIR/runtime-profiles-draft.md" \
  "$DIST_DIR/runtime-profiles.html" \
  "Runtime Profiles Draft" \
  "style.css"

build_page \
  "$SRC_DIR/execution-plan.md" \
  "$DIST_DIR/execution-plan.html" \
  "Multi-Agent Execution Plan" \
  "style.css"

build_page \
  "$SRC_DIR/execution-tracker.md" \
  "$DIST_DIR/execution-tracker.html" \
  "Execution Tracker" \
  "style.css"

build_page \
  "$SRC_DIR/go-live-checklist.md" \
  "$DIST_DIR/go-live-checklist.html" \
  "Go-Live Checklist" \
  "style.css"

# Build epic pages
mkdir -p "$DIST_DIR/epics"
for epic in "$SRC_DIR"/epics/*.md; do
  basename="$(basename "$epic" .md)"
  # Extract title from first H1 line, falling back to filename
  title="$(head -1 "$epic" | sed -E 's/^#+ *//')"
  pandoc "$epic" \
    "${PANDOC_OPTS[@]}" \
    --css ../style.css \
    --metadata home-link="../index.html" \
    --metadata title="$title" \
    -o "$DIST_DIR/epics/${basename}.html"
done

# Build runbook pages
mkdir -p "$DIST_DIR/runbooks"
for runbook in "$SRC_DIR"/runbooks/*.md; do
  basename="$(basename "$runbook" .md)"
  title="$(head -1 "$runbook" | sed -E 's/^#+ *//')"
  build_page \
    "$runbook" \
    "$DIST_DIR/runbooks/${basename}.html" \
    "$title" \
    "../style.css" \
    --metadata home-link="../index.html"
done

# Rewrite .md links to .html so cross-doc links resolve correctly
for html in "$DIST_DIR"/*.html "$DIST_DIR"/epics/*.html "$DIST_DIR"/runbooks/*.html; do
  if [ -f "$html" ]; then
    if [[ "$(uname -s)" == "Darwin" ]]; then
      sed -i '' 's/\.md"/.html"/g; s/\.md#/.html#/g' "$html"
    else
      sed -i 's/\.md"/.html"/g; s/\.md#/.html#/g' "$html"
    fi
  fi
done

GENERATED_AT="$(date -u +"%Y-%m-%d %H:%M:%S UTC")"

cat > "$DIST_DIR/index.html" <<EOF
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Conman Docs</title>
    <link rel="stylesheet" href="style.css" />
  </head>
  <body>
    <div class="landing">
      <img src="conman.svg" alt="Conman" class="landing-logo" />
      <h1>Conman Docs</h1>
      <p class="subtitle">Documentation for Conman v1 planning &amp; specification.</p>
      <div class="doc-cards">
        <a class="doc-card" href="./conman-v1-scope.html">
          <p class="card-title">V1 Scope Specification</p>
          <p class="card-desc">Architecture, features, and technical scope for the first release.</p>
        </a>
        <a class="doc-card" href="./conman-v1-backlog.html">
          <p class="card-title">V1 Implementation Backlog</p>
          <p class="card-desc">Prioritized tasks and milestones for the v1 build.</p>
        </a>
        <a class="doc-card" href="./implementation.html">
          <p class="card-title">V1 Implementation Guide</p>
          <p class="card-desc">Epic-by-epic implementation plan with ordered checklists.</p>
        </a>
        <a class="doc-card" href="./runtime-profiles.html">
          <p class="card-title">Runtime Profiles Draft</p>
          <p class="card-desc">Profile model for URLs, env vars, secrets, database, and data lifecycle.</p>
        </a>
        <a class="doc-card" href="./execution-plan.html">
          <p class="card-title">Execution Plan</p>
          <p class="card-desc">Dependency-ordered multi-agent delivery plan by wave and milestone.</p>
        </a>
        <a class="doc-card" href="./execution-tracker.html">
          <p class="card-title">Execution Tracker</p>
          <p class="card-desc">Live milestone gate status, blockers, and launch readiness sign-off.</p>
        </a>
        <a class="doc-card" href="./go-live-checklist.html">
          <p class="card-title">Go-Live Checklist</p>
          <p class="card-desc">Operational launch checklist with links to evidence artifacts.</p>
        </a>
        <a class="doc-card" href="./runbooks/REVIEW-SIGNOFF.html">
          <p class="card-title">Runbook Sign-Off</p>
          <p class="card-desc">On-call review checklist and runbook index for production readiness.</p>
        </a>
      </div>
      <p class="meta">Generated: ${GENERATED_AT}</p>
    </div>
  </body>
</html>
EOF

echo "Built docs site at: $DIST_DIR"
