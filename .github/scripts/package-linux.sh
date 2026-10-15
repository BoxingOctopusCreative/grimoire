#!/usr/bin/env bash
# Build a Grimoire tarball, .deb, and .rpm from a release binary.
# Usage: package-linux.sh <binary> <output-dir> <arch-label>
#
# arch-label is linux_x64 or linux_arm64, matching the other release assets.
# The packages install /usr/bin/grimoire, a desktop entry, and hicolor icons.
# Linked libraries become package dependencies. Wayland and Vulkan are dlopen'd,
# so those dependencies are added explicitly.
set -euo pipefail

BIN="${1:?path to the Grimoire binary}"
OUT="${2:?output directory}"
ARCH="${3:?arch label, such as linux_x64}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP_ID="com.boxingoctopus.grimoire"
MAINTAINER="Boxing Octopus Creative Engineering <ryan.draga@boxingoctop.us>"
HOMEPAGE="https://grimoirebooks.app"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/app/Cargo.toml" | head -1)"
if [[ -z "${VERSION}" ]]; then
  echo "Could not read version from app/Cargo.toml" >&2
  exit 1
fi
if [[ ! -f "${BIN}" ]]; then
  echo "Missing binary: ${BIN}" >&2
  exit 1
fi

case "${ARCH}" in
  linux_x64)
    DEB_ARCH="amd64"
    RPM_ARCH="x86_64"
    MACHINE_PAT="X86-64"
    ;;
  linux_arm64)
    DEB_ARCH="arm64"
    RPM_ARCH="aarch64"
    MACHINE_PAT="AArch64"
    ;;
  *)
    echo "Unknown arch label: ${ARCH} (expected linux_x64 or linux_arm64)" >&2
    exit 1
    ;;
esac

missing=()
for cmd in readelf objdump dpkg-deb dpkg-shlibdeps rpmbuild; do
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    missing+=("${cmd}")
  fi
done
if [[ ${#missing[@]} -gt 0 ]]; then
  echo "Missing packaging tools: ${missing[*]}" >&2
  echo "On Ubuntu: sudo apt-get install binutils dpkg-dev rpm desktop-file-utils" >&2
  exit 1
fi

HEADER="$(readelf -h "${BIN}")"
if ! grep -q 'ELF64' <<<"${HEADER}"; then
  echo "Not a 64-bit ELF binary: ${BIN}" >&2
  exit 1
fi
if ! grep -q "${MACHINE_PAT}" <<<"${HEADER}"; then
  echo "Binary machine does not match ${ARCH}:" >&2
  echo "${HEADER}" >&2
  exit 1
fi

DATE="$(date -u +%F)"
STAGE="$(mktemp -d "${TMPDIR:-/tmp}/grimoire-linux.XXXXXX")"
PAYLOAD="${STAGE}/root"
DEB_ROOT="${STAGE}/deb"

cleanup() {
  rm -rf "${STAGE}"
}
trap cleanup EXIT

install_icon() {
  local size="$1"
  local src="$2"
  install -D -m 0644 "${src}" \
    "${PAYLOAD}/usr/share/icons/hicolor/${size}/apps/${APP_ID}.png"
}

install -D -m 0755 "${BIN}" "${PAYLOAD}/usr/bin/grimoire"
install -D -m 0644 "${ROOT}/packaging/linux/${APP_ID}.desktop" \
  "${PAYLOAD}/usr/share/applications/${APP_ID}.desktop"
sed \
  -e "s/@VERSION@/${VERSION}/g" \
  -e "s/@DATE@/${DATE}/g" \
  "${ROOT}/packaging/linux/${APP_ID}.metainfo.xml" \
  > "${STAGE}/metainfo.xml"
install -D -m 0644 "${STAGE}/metainfo.xml" \
  "${PAYLOAD}/usr/share/metainfo/${APP_ID}.metainfo.xml"
install -D -m 0644 "${ROOT}/LICENSE" \
  "${PAYLOAD}/usr/share/licenses/grimoire/MPL-2.0"
install -D -m 0644 "${ROOT}/LICENSE" \
  "${PAYLOAD}/usr/share/doc/grimoire/copyright"

install_icon 32x32 "${ROOT}/app/icons/32x32.png"
install_icon 64x64 "${ROOT}/app/icons/64x64.png"
install_icon 128x128 "${ROOT}/app/icons/128x128.png"
install_icon 256x256 "${ROOT}/app/icons/128x128@2x.png"
install_icon 512x512 "${ROOT}/app/icons/icon.png"

if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "${PAYLOAD}/usr/share/applications/${APP_ID}.desktop"
fi

mkdir -p "${OUT}"
TAR="${OUT}/Grimoire_${VERSION}_${ARCH}.tar.gz"
rm -f "${TAR}"
tar -C "$(dirname "${BIN}")" -czf "${TAR}" "$(basename "${BIN}")"

# Versioned Depends from the binary's DT_NEEDED entries, plus libraries the
# process loads with dlopen (they never show up in NEEDED).
# debian/ is only for dpkg-shlibdeps. dpkg-deb packages DEBIAN/, not debian/.
mkdir -p "${DEB_ROOT}/debian"
cp -a "${PAYLOAD}/." "${DEB_ROOT}/"
cat > "${DEB_ROOT}/debian/control" <<EOF
Source: grimoire
Maintainer: ${MAINTAINER}

Package: grimoire
Architecture: ${DEB_ARCH}
Description: eBook collection manager
 placeholder
EOF
SHLIB_ERR="${STAGE}/shlibdeps.err"
SHLIBS="$(
  cd "${DEB_ROOT}"
  dpkg-shlibdeps -O usr/bin/grimoire 2>"${SHLIB_ERR}"
)" || {
  cat "${SHLIB_ERR}" >&2
  exit 1
}
# Expected when the binary is not staged under debian/<package>/.
if [[ -s "${SHLIB_ERR}" ]]; then
  grep -v "binaries to analyze should already be installed in their package's directory" \
    "${SHLIB_ERR}" >&2 || true
fi
DEPENDS="$(sed -n 's/^shlibs:Depends=//p' <<<"${SHLIBS}")"
if [[ -z "${DEPENDS}" ]]; then
  echo "dpkg-shlibdeps did not report dependencies:" >&2
  echo "${SHLIBS}" >&2
  exit 1
fi

add_dep() {
  local pkg="$1"
  case ", ${DEPENDS}," in
    *", ${pkg},"* | *", ${pkg} ("*) return ;;
  esac
  DEPENDS="${DEPENDS}, ${pkg}"
}

add_dep "libwayland-client0"
add_dep "libvulkan1"
add_dep "hicolor-icon-theme"

if command -v apt-cache >/dev/null 2>&1; then
  while IFS= read -r pkg; do
    pkg="${pkg%% (*}"
    pkg="$(echo "${pkg}" | tr -d ' ')"
    [[ -z "${pkg}" ]] && continue
    if ! apt-cache show "${pkg}" >/dev/null 2>&1; then
      echo "Debian package not found: ${pkg}" >&2
      exit 1
    fi
  done < <(echo "${DEPENDS}" | tr ',' '\n')
fi

rm -rf "${DEB_ROOT}/debian"
INSTALLED_SIZE="$(du -sk "${PAYLOAD}" | awk '{print $1}')"
mkdir -p "${DEB_ROOT}/DEBIAN"
cat > "${DEB_ROOT}/DEBIAN/control" <<EOF
Package: grimoire
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${DEB_ARCH}
Maintainer: ${MAINTAINER}
Homepage: ${HOMEPAGE}
Installed-Size: ${INSTALLED_SIZE}
Depends: ${DEPENDS}
Recommends: fonts-liberation | fonts-dejavu-core
Description: eBook collection manager
 Grimoire keeps a local library of EPUB, PDF, MOBI, AZW3, and comics,
 and sends books to Kindles and other eReaders.
EOF

cat > "${DEB_ROOT}/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q /usr/share/icons/hicolor || true
fi
EOF
cp "${DEB_ROOT}/DEBIAN/postinst" "${DEB_ROOT}/DEBIAN/postrm"
chmod 0755 "${DEB_ROOT}/DEBIAN/postinst" "${DEB_ROOT}/DEBIAN/postrm"

DEB="${OUT}/Grimoire_${VERSION}_${ARCH}.deb"
rm -f "${DEB}"
dpkg-deb --root-owner-group --build "${DEB_ROOT}" "${DEB}" >/dev/null

FILELIST="${STAGE}/files.list"
{
  echo "%dir /usr/share/doc/grimoire"
  echo "%dir /usr/share/licenses/grimoire"
  find "${PAYLOAD}/usr" -type f | sed "s|^${PAYLOAD}||" | sort
} > "${FILELIST}"

SPEC="${STAGE}/grimoire.spec"
sed \
  -e "s|@VERSION@|${VERSION}|g" \
  -e "s|@RPM_ARCH@|${RPM_ARCH}|g" \
  -e "s|@PAYLOAD@|${PAYLOAD}|g" \
  -e "s|@FILELIST@|${FILELIST}|g" \
  "${ROOT}/packaging/linux/grimoire.spec.in" > "${SPEC}"

RPM_TOP="${STAGE}/rpm"
mkdir -p "${RPM_TOP}/BUILD" "${RPM_TOP}/RPMS" "${RPM_TOP}/SOURCES" "${RPM_TOP}/SPECS" "${RPM_TOP}/SRPMS"
rpmbuild --define "_topdir ${RPM_TOP}" -bb "${SPEC}"
mapfile -t BUILT_RPMS < <(find "${RPM_TOP}/RPMS" -type f -name '*.rpm' | sort)
if [[ ${#BUILT_RPMS[@]} -ne 1 ]]; then
  echo "rpmbuild produced ${#BUILT_RPMS[@]} rpm files, expected 1" >&2
  printf '  %s\n' "${BUILT_RPMS[@]}" >&2
  exit 1
fi
RPM="${OUT}/Grimoire_${VERSION}_${ARCH}.rpm"
rm -f "${RPM}"
cp "${BUILT_RPMS[0]}" "${RPM}"

echo "Wrote ${TAR}"
echo "Wrote ${DEB}"
echo "Wrote ${RPM}"
