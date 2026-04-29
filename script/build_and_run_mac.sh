#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="DevCleaner"
PACKAGE_DIR="$ROOT/macos/DevCleanerMac"
DIST_DIR="$ROOT/dist"
BUNDLE="$DIST_DIR/$APP_NAME.app"
LOGO_PNG="$ROOT/logo.png"
VERIFY=0
BUILD_ONLY=0

for arg in "$@"; do
  case "$arg" in
    --verify) VERIFY=1 ;;
    --build-only) BUILD_ONLY=1 ;;
    *) echo "Unknown argument: $arg" >&2; exit 2 ;;
  esac
done

echo "Building Rust helper..."
cargo build --manifest-path "$ROOT/Cargo.toml"

echo "Building SwiftUI app..."
swift build --package-path "$PACKAGE_DIR"

SWIFT_BIN="$PACKAGE_DIR/.build/debug/DevCleanerMac"
HELPER_BIN="$ROOT/target/debug/dev-cleaner"

rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"
cp "$SWIFT_BIN" "$BUNDLE/Contents/MacOS/$APP_NAME"
cp "$HELPER_BIN" "$BUNDLE/Contents/Resources/dev-cleaner-helper"

if [[ -f "$LOGO_PNG" ]]; then
  ICONSET="$DIST_DIR/AppIcon.iconset"
  rm -rf "$ICONSET"
  mkdir -p "$ICONSET"
  sips -z 16 16 "$LOGO_PNG" --out "$ICONSET/icon_16x16.png" >/dev/null
  sips -z 32 32 "$LOGO_PNG" --out "$ICONSET/icon_16x16@2x.png" >/dev/null
  sips -z 32 32 "$LOGO_PNG" --out "$ICONSET/icon_32x32.png" >/dev/null
  sips -z 64 64 "$LOGO_PNG" --out "$ICONSET/icon_32x32@2x.png" >/dev/null
  sips -z 128 128 "$LOGO_PNG" --out "$ICONSET/icon_128x128.png" >/dev/null
  sips -z 256 256 "$LOGO_PNG" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
  sips -z 256 256 "$LOGO_PNG" --out "$ICONSET/icon_256x256.png" >/dev/null
  sips -z 512 512 "$LOGO_PNG" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
  sips -z 512 512 "$LOGO_PNG" --out "$ICONSET/icon_512x512.png" >/dev/null
  sips -z 1024 1024 "$LOGO_PNG" --out "$ICONSET/icon_512x512@2x.png" >/dev/null
  iconutil -c icns "$ICONSET" -o "$BUNDLE/Contents/Resources/AppIcon.icns"
  rm -rf "$ICONSET"
fi

cat > "$BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>$APP_NAME</string>
  <key>CFBundleIdentifier</key>
  <string>com.sian.devcleaner</string>
  <key>CFBundleName</key>
  <string>Dev Cleaner</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.2.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

if [[ "$BUILD_ONLY" == "1" ]]; then
  echo "Built $BUNDLE"
  exit 0
fi

pkill -x "$APP_NAME" >/dev/null 2>&1 || true
/usr/bin/open -n "$BUNDLE"

if [[ "$VERIFY" == "1" ]]; then
  sleep 2
  if pgrep -x "$APP_NAME" >/dev/null; then
    echo "Verified $APP_NAME is running."
  else
    echo "Failed to verify $APP_NAME process." >&2
    exit 1
  fi
fi
