#!/usr/bin/env bash
# Bump Grimoire version files from a PR title.
# Usage: bump-version.sh "<pr title>"
# Prints: VERSION=<semver> TAG=v<semver> BUMP=<major|minor>
set -euo pipefail

TITLE="${1:-}"
if [[ -z "${TITLE}" ]]; then
  echo "usage: bump-version.sh \"<pr title>\"" >&2
  exit 2
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

if [[ "${TITLE}" =~ ^[[:space:]]*[Mm][Aa][Jj][Oo][Rr]([[:space:]]|$) ]]; then
  BUMP="major"
elif [[ "${TITLE}" =~ ^[[:space:]]*[Mm][Ii][Nn][Oo][Rr]([[:space:]]|$) ]]; then
  BUMP="minor"
else
  echo "PR title must begin with MAJOR or MINOR (got: ${TITLE})" >&2
  exit 1
fi

CURRENT=""
if LATEST_TAG="$(git describe --tags --match 'v*' --abbrev=0 2>/dev/null || true)" \
  && [[ -n "${LATEST_TAG}" ]]; then
  CURRENT="${LATEST_TAG#v}"
elif [[ -f package.json ]]; then
  CURRENT="$(node -pe 'require("./package.json").version')"
else
  CURRENT="0.0.0"
fi

IFS='.' read -r MAJOR MINOR PATCH <<<"${CURRENT}"
MAJOR="${MAJOR:-0}"
MINOR="${MINOR:-0}"
PATCH="${PATCH:-0}"

case "${BUMP}" in
  major)
    MAJOR=$((MAJOR + 1))
    MINOR=0
    PATCH=0
    ;;
  minor)
    MINOR=$((MINOR + 1))
    PATCH=0
    ;;
esac

VERSION="${MAJOR}.${MINOR}.${PATCH}"
TAG="v${VERSION}"

node -e "
const fs = require('fs');
const path = 'package.json';
const pkg = JSON.parse(fs.readFileSync(path, 'utf8'));
pkg.version = process.argv[1];
fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + '\n');
" "${VERSION}"

node -e "
const fs = require('fs');
const path = 'src-tauri/tauri.conf.json';
const conf = JSON.parse(fs.readFileSync(path, 'utf8'));
conf.version = process.argv[1];
fs.writeFileSync(path, JSON.stringify(conf, null, 2) + '\n');
" "${VERSION}"

# Replace only the package version line in Cargo.toml.
perl -i -0pe 's/(?m)^(\[package\]\s*(?:(?!\[).)*?^version\s*=\s*)"[^"]*"/${1}"'"${VERSION}"'"/s' src-tauri/Cargo.toml

# Keep Cargo.lock package stanza in sync when present.
if [[ -f src-tauri/Cargo.lock ]]; then
  perl -i -0pe 's/(name = "grimoire"\nversion = )"[^"]*"/${1}"'"${VERSION}"'"/' src-tauri/Cargo.lock
fi

echo "VERSION=${VERSION}"
echo "TAG=${TAG}"
echo "BUMP=${BUMP}"
echo "PREVIOUS=${CURRENT}"
