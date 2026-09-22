#!/usr/bin/env bash
# Inject Gatekeeper installer files into built macOS DMGs.
# Usage: inject-macos-dmg-installer.sh [dmg-path...]
# With no args, finds DMGs under src-tauri/target/**/bundle/dmg/
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
INSTALLER_SRC="$ROOT/src-tauri/macos/Install Grimoire.command"
HOWTO_SRC="$ROOT/src-tauri/macos/How to install.txt"

if [[ ! -f "$INSTALLER_SRC" ]]; then
  echo "missing installer: $INSTALLER_SRC" >&2
  exit 1
fi
if [[ ! -f "$HOWTO_SRC" ]]; then
  echo "missing howto: $HOWTO_SRC" >&2
  exit 1
fi

if [[ $# -gt 0 ]]; then
  DMGS=("$@")
else
  # shellcheck disable=SC2207
  DMGS=($(find "$ROOT/src-tauri/target" -type f -path '*/bundle/dmg/*.dmg' ! -name 'rw.*.dmg' 2>/dev/null | sort -u))
fi

if [[ ${#DMGS[@]} -eq 0 ]]; then
  echo "No DMGs found to patch." >&2
  exit 1
fi

inject_one() {
  local dmg="$1"
  local work rw mount staging out

  echo "Injecting installer into $(basename "$dmg")…"

  work="$(mktemp -d "${TMPDIR:-/tmp}/grimoire-dmg.XXXXXX")"
  rw="$work/rw.dmg"
  staging="$work/mount"
  out="$work/out.dmg"
  mkdir -p "$staging"

  # Writable copy with headroom for installer files.
  hdiutil convert "$dmg" -format UDRW -o "$rw" >/dev/null
  # Resize: current size + 2MB (hdiutil wants sector counts; -size accepts m).
  local size_mb
  size_mb="$(du -sm "$rw" | awk '{print $1 + 2}')"
  hdiutil resize -size "${size_mb}m" "$rw" >/dev/null

  hdiutil attach "$rw" -readwrite -nobrowse -mountpoint "$staging" >/dev/null

  cp "$INSTALLER_SRC" "$staging/Install Grimoire.command"
  chmod +x "$staging/Install Grimoire.command"
  cp "$HOWTO_SRC" "$staging/How to install.txt"

  # Flush before detach.
  sync
  hdiutil detach "$staging" >/dev/null

  hdiutil convert "$rw" -format UDZO -imagekey zlib-level=9 -o "$out" >/dev/null
  mv -f "$out" "$dmg"
  rm -rf "$work"
  echo "  ok: $dmg"
}

for dmg in "${DMGS[@]}"; do
  if [[ ! -f "$dmg" ]]; then
    echo "skip missing: $dmg" >&2
    continue
  fi
  inject_one "$dmg"
done
