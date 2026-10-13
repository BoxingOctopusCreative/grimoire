#!/usr/bin/env bash
# Build a Grimoire.app and a DMG from a release binary.
# Usage: package-macos.sh <binary> <output-dir> <arch-label>
#
# Signing: when APPLE_CERTIFICATE is set, import that .p12 and sign with the
# Developer ID Application identity inside it. Otherwise use
# APPLE_SIGNING_IDENTITY from the login keychain. Notarize when the App Store
# Connect API key variables are set.
set -euo pipefail

BIN="${1:?path to the Grimoire binary}"
OUT="${2:?output directory}"
ARCH="${3:?arch label, such as macos_arm64}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/app/Cargo.toml" | head -1)"
if [[ -z "${VERSION}" ]]; then
  echo "Could not read version from app/Cargo.toml" >&2
  exit 1
fi
if [[ ! -f "${BIN}" ]]; then
  echo "Missing binary: ${BIN}" >&2
  exit 1
fi

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/grimoire-app.XXXXXX")"
KEYCHAIN_PATH=""
PREVIOUS_KEYCHAINS=""
MOUNTED=""

cleanup() {
  if [[ -n "${MOUNTED}" ]]; then
    hdiutil detach "${MOUNTED}" >/dev/null 2>&1 || hdiutil detach -force "${MOUNTED}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${KEYCHAIN_PATH}" ]]; then
    security delete-keychain "${KEYCHAIN_PATH}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${PREVIOUS_KEYCHAINS}" ]]; then
    # shellcheck disable=SC2086
    security list-keychains -d user -s ${PREVIOUS_KEYCHAINS} >/dev/null 2>&1 || true
  fi
  rm -rf "${STAGE}"
}
trap cleanup EXIT

APP="${STAGE}/Grimoire.app"
mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources"
cp "${BIN}" "${APP}/Contents/MacOS/Grimoire"
chmod +x "${APP}/Contents/MacOS/Grimoire"
cp "${ROOT}/app/icons/icon.icns" "${APP}/Contents/Resources/icon.icns"

cat > "${APP}/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>Grimoire</string>
  <key>CFBundleExecutable</key>
  <string>Grimoire</string>
  <key>CFBundleIconFile</key>
  <string>icon</string>
  <key>CFBundleIdentifier</key>
  <string>com.boxingoctopus.grimoire</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Grimoire</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

IDENTITY=""
if [[ -n "${APPLE_CERTIFICATE:-}" && -n "${APPLE_CERTIFICATE_PASSWORD:-}" ]]; then
  KEYCHAIN_PATH="${STAGE}/build.keychain-db"
  KEYCHAIN_PASSWORD="$(openssl rand -base64 32)"
  CERT_PATH="${STAGE}/certificate.p12"
  printf '%s' "${APPLE_CERTIFICATE}" | base64 -D > "${CERT_PATH}"
  PREVIOUS_KEYCHAINS="$(security list-keychains -d user | tr -d '"' | tr '\n' ' ')"
  security create-keychain -p "${KEYCHAIN_PASSWORD}" "${KEYCHAIN_PATH}"
  security set-keychain-settings -lut 21600 "${KEYCHAIN_PATH}"
  security unlock-keychain -p "${KEYCHAIN_PASSWORD}" "${KEYCHAIN_PATH}"
  security import "${CERT_PATH}" \
    -k "${KEYCHAIN_PATH}" \
    -P "${APPLE_CERTIFICATE_PASSWORD}" \
    -A -t cert -f pkcs12 \
    -T /usr/bin/codesign \
    -T /usr/bin/security
  security set-key-partition-list \
    -S apple-tool:,apple:,codesign: \
    -s -k "${KEYCHAIN_PASSWORD}" \
    "${KEYCHAIN_PATH}"
  security list-keychains -d user -s "${KEYCHAIN_PATH}" ${PREVIOUS_KEYCHAINS}
  IDENTITY="$(security find-identity -v -p codesigning "${KEYCHAIN_PATH}" | awk -F'"' '/Developer ID Application/{print $2; exit}')"
elif [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  IDENTITY="${APPLE_SIGNING_IDENTITY}"
fi

if [[ "${CI:-}" == "true" && -z "${IDENTITY}" ]]; then
  echo "Could not find a Developer ID Application identity in APPLE_CERTIFICATE." >&2
  exit 1
fi

if [[ -n "${IDENTITY}" ]]; then
  echo "Signing with ${IDENTITY}"
  codesign --force --options runtime --timestamp --sign "${IDENTITY}" "${APP}/Contents/MacOS/Grimoire"
  codesign --force --options runtime --timestamp --sign "${IDENTITY}" "${APP}"
fi

# Window matches the background image. Icon positions are centers:
# Grimoire.app on the left, Applications on the right.
DMG_WIDTH=680
DMG_HEIGHT=480
DMG_ICON_SIZE=128
DMG_APP_X=180
DMG_APP_Y=330
DMG_APPLICATIONS_X=500
DMG_APPLICATIONS_Y=330

write_styled_dmg() {
  local root="$1"
  local dest="$2"
  local rw="${STAGE}/grimoire-rw.dmg"
  local logo_dir="${STAGE}/logo"
  local logo="${logo_dir}/grimoire_logo-wide-dark.svg.png"
  local volname

  mkdir -p "${logo_dir}"
  # Quick Look renders the wide SVG. The background script crops the square padding.
  qlmanage -t -s 3200 -o "${logo_dir}" "${ROOT}/static/grimoire_logo-wide-dark.svg" >/dev/null
  if [[ ! -f "${logo}" ]]; then
    echo "Could not render static/grimoire_logo-wide-dark.svg" >&2
    exit 1
  fi

  hdiutil create -size 240m -fs HFS+ -volname Grimoire -ov "${rw}" >/dev/null
  # An open Grimoire disk takes /Volumes/Grimoire. The new image would then
  # mount at "/Volumes/Grimoire 1", and Finder would style the wrong disk.
  if [[ -d /Volumes/Grimoire ]]; then
    echo "Ejecting the open Grimoire disk so the new image can use that name."
    hdiutil detach "/Volumes/Grimoire" >/dev/null 2>&1 \
      || hdiutil detach -force "/Volumes/Grimoire" >/dev/null 2>&1 \
      || true
  fi
  # Mount under /Volumes so Finder sees the volume name. A private mountpoint
  # shows up under the folder name instead, and the window styling cannot find it.
  local attach_plist="${STAGE}/attach.plist"
  hdiutil attach "${rw}" -noverify -noautoopen -plist > "${attach_plist}"
  MOUNTED="$(python3 -c '
import plistlib, sys
info = plistlib.load(open(sys.argv[1], "rb"))
for entity in info.get("system-entities", []):
    point = entity.get("mount-point")
    if point:
        print(point)
        raise SystemExit
raise SystemExit("mounted image has no mount point")
' "${attach_plist}")"
  if [[ "${MOUNTED}" != "/Volumes/Grimoire" ]]; then
    echo "The disk image mounted at ${MOUNTED} instead of /Volumes/Grimoire." >&2
    echo "Eject every open Grimoire disk, then package again." >&2
    exit 1
  fi
  volname="Grimoire"
  cp -R "${root}/." "${MOUNTED}/"
  mkdir -p "${MOUNTED}/.background"
  swift "${ROOT}/.github/scripts/dmg-background.swift" \
    "${logo}" \
    "${MOUNTED}/.background/background.png" \
    "${MOUNTED}/.background/background@2x.png" \
    "${DMG_WIDTH}" \
    "${DMG_HEIGHT}" \
    "${DMG_APP_X}" \
    "${DMG_APPLICATIONS_X}"

  osascript <<EOF
tell application "Finder"
  tell disk "${volname}"
    open
    delay 1
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 120, 200 + ${DMG_WIDTH}, 120 + ${DMG_HEIGHT}}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to ${DMG_ICON_SIZE}
    set text size of viewOptions to 13
    set background picture of viewOptions to file ".background:background.png"
    delay 1
    set position of item "Grimoire.app" of container window to {${DMG_APP_X}, ${DMG_APP_Y}}
    set position of item "Applications" of container window to {${DMG_APPLICATIONS_X}, ${DMG_APPLICATIONS_Y}}
    close
    open
    delay 1
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 120, 200 + ${DMG_WIDTH}, 120 + ${DMG_HEIGHT}}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to ${DMG_ICON_SIZE}
    set background picture of viewOptions to file ".background:background.png"
    set position of item "Grimoire.app" of container window to {${DMG_APP_X}, ${DMG_APP_Y}}
    set position of item "Applications" of container window to {${DMG_APPLICATIONS_X}, ${DMG_APPLICATIONS_Y}}
    update without registering applications
    delay 2
    close
  end tell
end tell
EOF

  sync
  hdiutil detach "${MOUNTED}" >/dev/null
  MOUNTED=""
  hdiutil convert "${rw}" -format UDZO -imagekey zlib-level=9 -o "${dest}" >/dev/null
}

DMG_ROOT="${STAGE}/dmg"
mkdir -p "${DMG_ROOT}"
cp -R "${APP}" "${DMG_ROOT}/"
ln -s /Applications "${DMG_ROOT}/Applications"

mkdir -p "${OUT}"
DMG="${OUT}/Grimoire_${VERSION}_${ARCH}.dmg"
rm -f "${DMG}"
write_styled_dmg "${DMG_ROOT}" "${DMG}"

if [[ -n "${IDENTITY}" ]]; then
  codesign --force --timestamp --sign "${IDENTITY}" "${DMG}"
fi

if [[ -n "${IDENTITY}" && -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" && -n "${APPLE_API_KEY_PATH:-}" ]]; then
  echo "Notarizing ${DMG}"
  xcrun notarytool submit "${DMG}" \
    --key "${APPLE_API_KEY_PATH}" \
    --key-id "${APPLE_API_KEY}" \
    --issuer "${APPLE_API_ISSUER}" \
    --wait
  xcrun stapler staple "${DMG}"
fi

echo "Wrote ${DMG}"
