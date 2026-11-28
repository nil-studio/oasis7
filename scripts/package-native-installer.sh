#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLATFORM=""
BUNDLE_DIR=""
OUT_DIR=""
ASSET_NAME=""
VERSION=""
DRY_RUN=0
APPIMAGE_RUNTIME_FILE="${OASIS7_APPIMAGE_RUNTIME_FILE:-}"

usage() {
  cat <<'USAGE'
Usage: ./scripts/package-native-installer.sh [options]

Package a prepared launcher bundle into a platform-native release artifact.

Options:
  --platform <id>      required: linux-x64 | macos-x64 | macos-arm64 | windows-x64
  --bundle-dir <path>  required: prepared bundle directory to package
  --out-dir <path>     required: output directory for packaged installer
  --asset-name <name>  required: final asset filename (.AppImage | .deb | .dmg | .exe)
  --version <value>    required: release version (for example 0.0.40)
  --appimage-runtime-file <path>
                       optional: pre-downloaded AppImage type2 runtime for Linux
  --dry-run            print commands only; do not execute
  -h, --help           show this help
USAGE
}

run() {
  echo "+ $*"
  if [[ "$DRY_RUN" == "1" ]]; then
    return 0
  fi
  "$@"
}

ensure_command() {
  local cmd="$1"
  if [[ "$DRY_RUN" == "1" ]]; then
    return 0
  fi
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "error: required command not found: $cmd" >&2
    exit 1
  fi
}

resolve_abs_path() {
  local input="$1"
  if [[ "$input" == /* ]]; then
    printf '%s\n' "$input"
  else
    printf '%s\n' "$ROOT_DIR/$input"
  fi
}

normalize_release_version() {
  local value="${1#v}"
  value="$(printf '%s' "$value" | tr ' ' '-')"
  value="$(printf '%s' "$value" | sed -E 's/[^A-Za-z0-9.+:~_-]+/-/g')"
  value="$(printf '%s' "$value" | sed -E 's/-+/-/g; s/^-+//; s/-+$//')"
  printf '%s\n' "$value"
}

normalize_deb_version() {
  local value
  value="$(normalize_release_version "$1")"
  value="$(printf '%s' "$value" | sed -E 's/_/~/g')"
  printf '%s\n' "$value"
}

require_bundle_path() {
  local path="$1"
  if [[ ! -e "$path" ]]; then
    echo "error: required bundle path missing: $path" >&2
    exit 1
  fi
}

windows_native_path() {
  local path="$1"
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -aw "$path"
  else
    printf '%s\n' "$path"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --platform)
      PLATFORM="${2:-}"
      shift 2
      ;;
    --bundle-dir)
      BUNDLE_DIR="${2:-}"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --asset-name)
      ASSET_NAME="${2:-}"
      shift 2
      ;;
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --appimage-runtime-file)
      APPIMAGE_RUNTIME_FILE="${2:-}"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

case "$PLATFORM" in
  linux-x64|macos-x64|macos-arm64|windows-x64) ;;
  *)
    echo "error: --platform must be one of linux-x64|macos-x64|macos-arm64|windows-x64" >&2
    exit 1
    ;;
esac

[[ -n "$BUNDLE_DIR" ]] || { echo "error: --bundle-dir is required" >&2; exit 1; }
[[ -n "$OUT_DIR" ]] || { echo "error: --out-dir is required" >&2; exit 1; }
[[ -n "$ASSET_NAME" ]] || { echo "error: --asset-name is required" >&2; exit 1; }
[[ -n "$VERSION" ]] || { echo "error: --version is required" >&2; exit 1; }

BUNDLE_DIR="$(resolve_abs_path "$BUNDLE_DIR")"
OUT_DIR="$(resolve_abs_path "$OUT_DIR")"
OUT_FILE="$OUT_DIR/$ASSET_NAME"
RELEASE_VERSION="$(normalize_release_version "$VERSION")"

if [[ -n "$APPIMAGE_RUNTIME_FILE" ]]; then
  APPIMAGE_RUNTIME_FILE="$(resolve_abs_path "$APPIMAGE_RUNTIME_FILE")"
fi

if [[ -z "$RELEASE_VERSION" ]]; then
  echo "error: normalized release version is empty: $VERSION" >&2
  exit 1
fi

if [[ ! -d "$BUNDLE_DIR" && "$DRY_RUN" != "1" ]]; then
  echo "error: --bundle-dir does not exist: $BUNDLE_DIR" >&2
  exit 1
fi

if [[ "$DRY_RUN" != "1" ]]; then
  "$ROOT_DIR/scripts/validate-release-platform-entrypoints.sh" \
    --platform "$PLATFORM" \
    --bundle-dir "$BUNDLE_DIR"
fi

run mkdir -p "$OUT_DIR"
run rm -f "$OUT_FILE"

case "$PLATFORM" in
  linux-x64)
    if [[ "$DRY_RUN" == "1" ]]; then
      TMP_DIR="$OUT_DIR/.dry-run-linux-x64"
    else
      TMP_DIR="$(mktemp -d)"
      trap 'rm -rf "$TMP_DIR"' EXIT
    fi
    if [[ "$ASSET_NAME" == *.deb ]]; then
      ensure_command dpkg-deb
      PACKAGE_ROOT="$TMP_DIR/package"
      INSTALL_ROOT="$PACKAGE_ROOT/opt/oasis7"
      DEBIAN_DIR="$PACKAGE_ROOT/DEBIAN"
      BIN_DIR="$PACKAGE_ROOT/usr/bin"
      DEB_VERSION="$(normalize_deb_version "$VERSION")"

      run mkdir -p "$INSTALL_ROOT" "$DEBIAN_DIR" "$BIN_DIR"
      run cp -R "$BUNDLE_DIR/." "$INSTALL_ROOT/"

      if [[ "$DRY_RUN" == "1" ]]; then
        echo "+ write $DEBIAN_DIR/control"
        echo "+ write launcher symlinks under $BIN_DIR"
        echo "+ dpkg-deb --build --root-owner-group '$PACKAGE_ROOT' '$OUT_FILE'"
        exit 0
      fi

      cat > "$DEBIAN_DIR/control" <<EOF
Package: oasis7
Version: $DEB_VERSION
Section: games
Priority: optional
Architecture: amd64
Maintainer: oasis7 release automation <noreply@oasis7.invalid>
Description: oasis7 technical preview bundle
 A platform preview bundle for oasis7 release validation and local technical
 verification.
EOF

      ln -s /opt/oasis7/run-client.sh "$BIN_DIR/oasis7-client"
      ln -s /opt/oasis7/run-game.sh "$BIN_DIR/oasis7-game"
      ln -s /opt/oasis7/run-web-launcher.sh "$BIN_DIR/oasis7-web-launcher"
      ln -s /opt/oasis7/run-chain-runtime.sh "$BIN_DIR/oasis7-chain-runtime"

      dpkg-deb --build --root-owner-group "$PACKAGE_ROOT" "$OUT_FILE"
    elif [[ "$ASSET_NAME" == *.AppImage ]]; then
      ensure_command appimagetool
      require_bundle_path "$BUNDLE_DIR/run-client.sh"
      ICON_SRC="$ROOT_DIR/site/assets/images/favicon.svg"
      require_bundle_path "$ICON_SRC"
      APPIMAGETOOL_ARGS=(--no-appstream)
      if [[ -n "$APPIMAGE_RUNTIME_FILE" ]]; then
        require_bundle_path "$APPIMAGE_RUNTIME_FILE"
        APPIMAGETOOL_ARGS=(--runtime-file "$APPIMAGE_RUNTIME_FILE" "${APPIMAGETOOL_ARGS[@]}")
      fi
      APPDIR="$TMP_DIR/oasis7.AppDir"
      APP_ROOT="$APPDIR/usr/lib/oasis7"

      run mkdir -p "$APP_ROOT" "$APPDIR/usr/bin"
      run cp -R "$BUNDLE_DIR/." "$APP_ROOT/"

      if [[ "$DRY_RUN" == "1" ]]; then
        echo "+ write $APPDIR/AppRun"
        echo "+ write $APPDIR/oasis7.desktop"
        echo "+ copy $ICON_SRC -> $APPDIR/oasis7.svg"
        echo "+ ln -s oasis7.svg '$APPDIR/.DirIcon'"
        echo "+ ln -s ../lib/oasis7/run-client.sh '$APPDIR/usr/bin/oasis7'"
        echo "+ env ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 appimagetool ${APPIMAGETOOL_ARGS[*]} '$APPDIR' '$OUT_FILE'"
        exit 0
      fi

      cat > "$APPDIR/AppRun" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
APPDIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$APPDIR/usr/lib/oasis7/run-client.sh" "$@"
EOF
      chmod +x "$APPDIR/AppRun"

      cat > "$APPDIR/oasis7.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=oasis7
Comment=oasis7 technical preview launcher
Exec=AppRun
Icon=oasis7
Terminal=false
Categories=Game;
StartupNotify=true
X-AppImage-Version=$RELEASE_VERSION
EOF

      cp "$ICON_SRC" "$APPDIR/oasis7.svg"
      ln -s oasis7.svg "$APPDIR/.DirIcon"
      ln -s ../lib/oasis7/run-client.sh "$APPDIR/usr/bin/oasis7"

      env ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 appimagetool "${APPIMAGETOOL_ARGS[@]}" "$APPDIR" "$OUT_FILE"
    else
      echo "error: linux-x64 asset must end with .AppImage or .deb" >&2
      exit 1
    fi
    ;;
  macos-x64|macos-arm64)
    [[ "$ASSET_NAME" == *.dmg ]] || { echo "error: $PLATFORM asset must end with .dmg" >&2; exit 1; }
    ensure_command hdiutil
    if [[ "$DRY_RUN" == "1" ]]; then
      TMP_DIR="$OUT_DIR/.dry-run-$PLATFORM"
    else
      TMP_DIR="$(mktemp -d)"
      trap 'rm -rf "$TMP_DIR"' EXIT
    fi
    DMG_ROOT="$TMP_DIR/dmg-root"
    run mkdir -p "$DMG_ROOT"
    run cp -R "$BUNDLE_DIR/." "$DMG_ROOT/"
    run ln -s /Applications "$DMG_ROOT/Applications"
    run hdiutil create -volname "oasis7 $RELEASE_VERSION" -srcfolder "$DMG_ROOT" -ov -format UDZO "$OUT_FILE"
    ;;
  windows-x64)
    [[ "$ASSET_NAME" == *.exe ]] || { echo "error: windows-x64 asset must end with .exe" >&2; exit 1; }
    ensure_command pwsh
    ensure_command makensis
    require_bundle_path "$BUNDLE_DIR/run-client.cmd"
    require_bundle_path "$BUNDLE_DIR/bin/oasis7_game_launcher.exe"
    BUNDLE_DIR_NATIVE="$(windows_native_path "$BUNDLE_DIR")"
    OUT_FILE_NATIVE="$(windows_native_path "$OUT_FILE")"
    NSIS_SCRIPT_NATIVE="$(windows_native_path "$ROOT_DIR/scripts/windows-release-installer.nsi")"
    if [[ "$DRY_RUN" == "1" ]]; then
      echo "+ makensis /DBUNDLE_DIR='$BUNDLE_DIR_NATIVE' /DOUT_FILE='$OUT_FILE_NATIVE' /DRELEASE_VERSION='$RELEASE_VERSION' '$NSIS_SCRIPT_NATIVE'"
      exit 0
    fi

    export OASIS7_WINDOWS_BUNDLE_DIR="$BUNDLE_DIR_NATIVE"
    export OASIS7_WINDOWS_OUT_FILE="$OUT_FILE_NATIVE"
    export OASIS7_WINDOWS_RELEASE_VERSION="$RELEASE_VERSION"
    export OASIS7_WINDOWS_NSIS_SCRIPT="$NSIS_SCRIPT_NATIVE"
    pwsh -NoLogo -NoProfile -Command '
      $ErrorActionPreference = "Stop"
      $bundleDir = $env:OASIS7_WINDOWS_BUNDLE_DIR
      $outFile = $env:OASIS7_WINDOWS_OUT_FILE
      $nsisScript = $env:OASIS7_WINDOWS_NSIS_SCRIPT
      $releaseVersion = $env:OASIS7_WINDOWS_RELEASE_VERSION
      $parentDir = Split-Path -Parent $outFile
      if (-not (Test-Path $parentDir)) {
        New-Item -ItemType Directory -Path $parentDir | Out-Null
      }
      if (Test-Path $outFile) {
        Remove-Item $outFile -Force
      }
      & makensis "/DBUNDLE_DIR=$bundleDir" "/DOUT_FILE=$outFile" "/DRELEASE_VERSION=$releaseVersion" $nsisScript | Out-Host
      if ($LASTEXITCODE -ne 0) {
        throw "makensis failed with exit code $LASTEXITCODE"
      }
    '
    ;;
esac

echo "Packaged installer asset: $OUT_FILE"
