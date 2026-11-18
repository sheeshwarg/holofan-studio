#!/usr/bin/env bash
# Builds "HoloFan Studio.app" and "HoloFan-Studio-13.0.dmg" on a Mac
# (one universal app for Apple Silicon and Intel).
#
# Needs Rust: https://rustup.rs
#   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Then, in Terminal, from the app folder:
#   bash tools/build-macos.sh
# The results land in dist/. Open the .dmg and drag the app onto "Applications".
set -euo pipefail
cd "$(dirname "$0")/.."

NAME="HoloFan Studio"
VERSION="13.0"
APP="dist/$NAME.app"
DMG="dist/HoloFan-Studio-$VERSION.dmg"

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

rm -rf "$APP" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create \
  target/aarch64-apple-darwin/release/holofan_studio \
  target/x86_64-apple-darwin/release/holofan_studio \
  -output "$APP/Contents/MacOS/$NAME"

# App icon: the program draws its own icon; turn it into an .icns
ICONSET="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$ICONSET"
BIN="target/aarch64-apple-darwin/release/holofan_studio"
[ "$(uname -m)" = "x86_64" ] && BIN="target/x86_64-apple-darwin/release/holofan_studio"
for s in 16 32 128 256 512; do
  "$BIN" --write-icon "$ICONSET/icon_${s}x${s}.png" "$s"
  "$BIN" --write-icon "$ICONSET/icon_${s}x${s}@2x.png" "$((s * 2))"
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>app.holofanstudio.desktop</string>
  <key>CFBundleVersion</key><string>13.0.0</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>$NAME</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSRequiresAquaSystemAppearance</key><false/>
  <key>NSLocalNetworkUsageDescription</key><string>HoloFan Studio connects to your hologram fan on the local network.</string>
  <key>NSRemovableVolumesUsageDescription</key><string>HoloFan Studio saves videos to the fan's SD card.</string>
</dict>
</plist>
PLIST

# Ad-hoc signature so Gatekeeper lets it start (right-click > Open the first time).
codesign --force --deep --sign - "$APP" || true

# Disk image with the app and a shortcut to Applications (drag to install).
STAGE="$(mktemp -d)/$NAME"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "$NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG"

echo
echo "Done:"
echo "  App: $APP"
echo "  DMG: $DMG"
