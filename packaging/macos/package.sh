#!/usr/bin/env bash
# Builds a release binary and assembles it into a proper Srotas Desk.app
# bundle — this is what makes macOS show the real logo in the Dock and
# Finder instead of a generic "exec" icon, which only happens for bare
# Unix binaries launched outside an app bundle.
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo build --release

APP="dist/macos/Srotas Desk.app"
rm -rf "dist/macos"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp target/release/srotas-desk "$APP/Contents/MacOS/srotas-desk"
cp packaging/macos/Info.plist "$APP/Contents/Info.plist"
cp assets/icon.icns "$APP/Contents/Resources/icon.icns"

echo "Built: $APP"
echo "Run it with: open \"$APP\""
