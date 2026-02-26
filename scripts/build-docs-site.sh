#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$ROOT_DIR/docs"
SITE_DIR="$ROOT_DIR/docs/site"
DIST_DIR="$SITE_DIR/dist"

mkdir -p "$DIST_DIR"

cp "$SITE_DIR/style.css" "$DIST_DIR/style.css"

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
  --css style.css
)

pandoc "$SRC_DIR/conman-v1-scope.md" \
  "${PANDOC_OPTS[@]}" \
  --metadata title="Conman V1 Scope" \
  -o "$DIST_DIR/conman-v1-scope.html"

pandoc "$SRC_DIR/conman-v1-backlog.md" \
  "${PANDOC_OPTS[@]}" \
  --metadata title="Conman V1 Backlog" \
  -o "$DIST_DIR/conman-v1-backlog.html"

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
      </div>
      <p class="meta">Generated: ${GENERATED_AT}</p>
    </div>
  </body>
</html>
EOF

echo "Built docs site at: $DIST_DIR"
