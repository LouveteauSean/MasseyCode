#!/usr/bin/env bash
# Build MasseyCode.app for macOS and zip it for distribution.
#
# Run on a Mac with the Xcode command line tools and Rust installed:
#
#     bash scripts/make-macos-app.sh
#
# The result is `dist/MasseyCode.app` and `dist/MasseyCode-macos.zip`.
# Set MASSEYCODES_SKIP_BUILD=1 to bundle an already built binary.

set -euo pipefail
cd "$(dirname "$0")/.."

APP="dist/MasseyCode.app"
BIN="target/release/masseycode"
ICON_SRC="assets/masseycode-1024.png"
ICONSET="build/masseycode.iconset"

if [[ "${MASSEYCODES_SKIP_BUILD:-0}" != "1" ]]; then
	cargo build -p massey-gui --release
fi

if [[ ! -f "$BIN" ]]; then
	echo "missing $BIN (build first)" >&2
	exit 1
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BIN" "$APP/Contents/MacOS/masseycode"
cp assets/Info.plist "$APP/Contents/Info.plist"

# A .icns built from the same artwork as the Windows .ico.
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
	sips -z "$size" "$size" "$ICON_SRC" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
	double=$((size * 2))
	sips -z "$double" "$double" "$ICON_SRC" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/MasseyCode.icns"
rm -rf "$ICONSET"

# Ad-hoc signature: enough for Gatekeeper to run it locally, no notarisation.
codesign --force --deep --sign - "$APP" >/dev/null

mkdir -p dist
rm -f dist/MasseyCode-macos.zip
# ditto keeps the bundle details that Finder expects.
ditto -c -k --sequesterRsrc --keepParent "$APP" dist/MasseyCode-macos.zip

echo "built $APP"
echo "built dist/MasseyCode-macos.zip"
