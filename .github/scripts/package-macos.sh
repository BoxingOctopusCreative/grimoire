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

cleanup() {
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

DMG_ROOT="${STAGE}/dmg"
mkdir -p "${DMG_ROOT}"
cp -R "${APP}" "${DMG_ROOT}/"
ln -s /Applications "${DMG_ROOT}/Applications"
if [[ -z "${IDENTITY}" ]]; then
  cp "${ROOT}/app/macos/Install Grimoire.command" "${DMG_ROOT}/Install Grimoire.command"
  chmod +x "${DMG_ROOT}/Install Grimoire.command"
  cp "${ROOT}/app/macos/How to install.txt" "${DMG_ROOT}/How to install.txt"
fi

mkdir -p "${OUT}"
DMG="${OUT}/Grimoire_${VERSION}_${ARCH}.dmg"
rm -f "${DMG}"
hdiutil create -volname Grimoire -srcfolder "${DMG_ROOT}" -ov -format UDZO "${DMG}" >/dev/null

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
