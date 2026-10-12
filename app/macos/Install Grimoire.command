#!/bin/bash
# Install Grimoire for macOS without an Apple Developer certificate.
# Clears Gatekeeper quarantine (the "damaged and can't be opened" message),
# copies the app to /Applications, and launches it.
set -euo pipefail

clear_quarantine() {
  local path="$1"
  if [[ -e "$path" ]]; then
    xattr -cr "$path" 2>/dev/null || true
  fi
}

here="$(cd "$(dirname "$0")" && pwd)"
# Allow Terminal to run this .command even when the DMG is quarantined.
clear_quarantine "$0"
clear_quarantine "$here"

app_src=""
if [[ -d "$here/Grimoire.app" ]]; then
  app_src="$here/Grimoire.app"
else
  echo "Could not find Grimoire.app next to this installer." >&2
  echo "Open the Grimoire .dmg, then double-click \"Install Grimoire.command\" inside it." >&2
  read -r -p "Press Return to close…" _
  exit 1
fi

dest="/Applications/Grimoire.app"

echo "Installing Grimoire…"
echo "  from: $app_src"
echo "  to:   $dest"
echo

clear_quarantine "$app_src"

if [[ -d "$dest" ]]; then
  echo "Removing previous install…"
  rm -rf "$dest"
fi

# ditto preserves bundle metadata better than cp -R on macOS.
ditto "$app_src" "$dest"
clear_quarantine "$dest"

echo
echo "Installed. Opening Grimoire…"
open "$dest"

echo
echo "Done. You can eject the disk image."
read -r -p "Press Return to close…" _
