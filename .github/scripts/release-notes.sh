#!/usr/bin/env bash
# Build release notes from git commit subjects since the previous version tag.
# Usage:
#   release-notes.sh <previous-tag>   # commits after previous-tag
#   release-notes.sh ""               # all commits (first release)
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: release-notes.sh <previous-tag-or-empty>" >&2
  exit 2
fi

PREV="${1}"

if [[ -n "${PREV}" ]] && git rev-parse "${PREV}" >/dev/null 2>&1; then
  RANGE="${PREV}..HEAD"
  HEADER="Changes since ${PREV}"
else
  RANGE="HEAD"
  HEADER="Changes"
fi

{
  echo "## ${HEADER}"
  echo
  git log "${RANGE}" --pretty=format:'- %s (%h)' --no-merges \
    | grep -vE '^- chore: release v' \
    || true
  echo
}
