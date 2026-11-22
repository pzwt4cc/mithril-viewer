#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

APP_ID="io.github.pzwt4cc.mithril-viewer"
BIN_NAME="mithril-viewer"
ARCH="${ARCH:-$(uname -m)}"
VERSION="${VERSION:-$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)}"
BUILD_DIR="${BUILD_DIR:-${ROOT_DIR}/target/appimage}"
APPDIR="${BUILD_DIR}/MithrilViewer.AppDir"
OUT_NAME="${OUT_NAME:-Mithril-Viewer-${VERSION}-${ARCH}.AppImage}"

CARGO_ARGS=(--release --locked)
DEFAULT_BIN_PATH="${ROOT_DIR}/target/release/${BIN_NAME}"
if [[ -n "${CARGO_TARGET:-}" ]]; then
  CARGO_ARGS+=(--target "$CARGO_TARGET")
  DEFAULT_BIN_PATH="${ROOT_DIR}/target/${CARGO_TARGET}/release/${BIN_NAME}"
fi
BIN_PATH="${BIN_PATH:-$DEFAULT_BIN_PATH}"

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build "${CARGO_ARGS[@]}"
fi

rm -rf "$APPDIR"
mkdir -p \
  "${APPDIR}/usr/bin" \
  "${APPDIR}/usr/lib" \
  "${APPDIR}/usr/share/applications" \
  "${APPDIR}/usr/share/metainfo" \
  "${APPDIR}/usr/share/licenses/${BIN_NAME}"

install -Dm755 "$BIN_PATH" "${APPDIR}/usr/bin/${BIN_NAME}"
install -Dm755 "${ROOT_DIR}/packaging/appimage/AppRun" "${APPDIR}/AppRun"

install -Dm644 "${ROOT_DIR}/data/${APP_ID}.desktop" "${APPDIR}/${APP_ID}.desktop"
install -Dm644 "${ROOT_DIR}/data/${APP_ID}.desktop" "${APPDIR}/usr/share/applications/${APP_ID}.desktop"
install -Dm644 "${ROOT_DIR}/data/${APP_ID}.metainfo.xml" "${APPDIR}/usr/share/metainfo/${APP_ID}.metainfo.xml"

install -Dm644 "${ROOT_DIR}/data/icons/hicolor/256x256/apps/${APP_ID}.png" "${APPDIR}/${APP_ID}.png"
ln -sf "${APP_ID}.png" "${APPDIR}/.DirIcon"

install -Dm644 "${ROOT_DIR}/data/icons/hicolor/scalable/apps/${APP_ID}.svg" \
  "${APPDIR}/usr/share/icons/hicolor/scalable/apps/${APP_ID}.svg"
for size in 16 24 32 48 64 128 256 512; do
  install -Dm644 "${ROOT_DIR}/data/icons/hicolor/${size}x${size}/apps/${APP_ID}.png" \
    "${APPDIR}/usr/share/icons/hicolor/${size}x${size}/apps/${APP_ID}.png"
done

install -Dm644 "${ROOT_DIR}/LICENSE" "${APPDIR}/usr/share/licenses/${BIN_NAME}/LICENSE"
install -Dm644 "${ROOT_DIR}/assets/fonts/OFL.txt" "${APPDIR}/usr/share/licenses/${BIN_NAME}/OFL.txt"

if dav1d_path="$(ldd "$BIN_PATH" | awk '/libdav1d\.so/ { print $3; exit }')" && [[ -n "$dav1d_path" && -e "$dav1d_path" ]]; then
  install -Dm755 "$(readlink -f "$dav1d_path")" "${APPDIR}/usr/lib/$(basename "$dav1d_path")"
  dav1d_soname="$(basename "$dav1d_path")"
  if [[ "$dav1d_soname" != "libdav1d.so.7" ]]; then
    ln -sf "$dav1d_soname" "${APPDIR}/usr/lib/libdav1d.so.7"
  fi
fi

APPIMAGETOOL="${APPIMAGETOOL:-}"
if [[ -z "$APPIMAGETOOL" ]]; then
  if command -v appimagetool >/dev/null 2>&1; then
    APPIMAGETOOL="$(command -v appimagetool)"
  else
    APPIMAGETOOL="${BUILD_DIR}/tools/appimagetool-${ARCH}.AppImage"
    if [[ ! -x "$APPIMAGETOOL" ]]; then
      mkdir -p "$(dirname "$APPIMAGETOOL")"
      curl -fsSL -o "$APPIMAGETOOL" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH}.AppImage"
      chmod +x "$APPIMAGETOOL"
    fi
  fi
fi

mkdir -p "$(dirname "${BUILD_DIR}/${OUT_NAME}")"
ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGETOOL" "$APPDIR" "${BUILD_DIR}/${OUT_NAME}"
echo "Created ${BUILD_DIR}/${OUT_NAME}"
