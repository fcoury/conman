#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SIGNOFF_FILE="$ROOT/docs/runbooks/REVIEW-SIGNOFF.md"

if [[ ! -f "$SIGNOFF_FILE" ]]; then
  echo "Sign-off file not found: $SIGNOFF_FILE" >&2
  exit 1
fi

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <reviewer-name> [YYYY-MM-DD]" >&2
  exit 1
fi

REVIEWER="$1"
DATE_VALUE="${2:-$(date -u +%Y-%m-%d)}"

tmp="$(mktemp)"
awk -v reviewer="$REVIEWER" -v date_value="$DATE_VALUE" '
  BEGIN { in_runbooks = 0 }
  /^Date:/ {
    print "Date: " date_value
    next
  }
  /^Reviewer:/ {
    print "Reviewer: " reviewer
    next
  }
  /^## Runbooks/ {
    in_runbooks = 1
    print
    next
  }
  /^## / && $0 !~ /^## Runbooks/ {
    in_runbooks = 0
    print
    next
  }
  {
    if (in_runbooks == 1) {
      gsub(/^- \[ \]/, "- [x]")
    }
    print
  }
' "$SIGNOFF_FILE" > "$tmp"

mv "$tmp" "$SIGNOFF_FILE"
echo "Updated runbook sign-off: $SIGNOFF_FILE"
