#!/usr/bin/env bash
#
# Builds a universal (Apple Silicon + Intel) rustafari.app and wraps it in a
# DMG. Signing and notarization are opt-in: set the env vars below and they run,
# leave them unset and you get an unsigned bundle that is fine for local use but
# that Gatekeeper will quarantine on other machines.
#
#   MACOS_SIGN_IDENTITY   e.g. "Developer ID Application: Your Name (TEAMID)"
#   MACOS_NOTARY_PROFILE  a `xcrun notarytool store-credentials` profile name
#
set -euo pipefail

cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
DIST=target/dist
APP="$DIST/rustafari.app"
DMG="$DIST/rustafari-$VERSION-macos.dmg"

# Cross-compiling needs a rustup-managed toolchain; a bare Homebrew cargo has
# std for the host target only. Resolve rustup's cargo explicitly, because it is
# not necessarily the one first on PATH.
if command -v rustup >/dev/null; then
	CARGO=$(rustup which cargo)
	# cargo finds rustc on PATH, which may well be a different install than the
	# cargo we just resolved, so pin the pair together.
	export RUSTC
	RUSTC=$(rustup which rustc)
else
	echo "!! rustup not found — falling back to \`cargo\`, which can only build for this machine's architecture"
	CARGO=cargo
fi

echo "==> Building rustafari $VERSION (universal)"
for target in aarch64-apple-darwin x86_64-apple-darwin; do
	if command -v rustup >/dev/null; then
		rustup target add "$target"
	fi
	"$CARGO" build --release --locked -p rustafari --target "$target"
done

echo "==> Assembling $APP"
rm -rf "$APP" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

lipo -create -output "$APP/Contents/MacOS/rustafari" \
	target/aarch64-apple-darwin/release/rustafari \
	target/x86_64-apple-darwin/release/rustafari

sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist >"$APP/Contents/Info.plist"

if [[ -f packaging/macos/rustafari.icns ]]; then
	cp packaging/macos/rustafari.icns "$APP/Contents/Resources/"
else
	# Not fatal: the app runs fine with the generic icon.
	echo "!! packaging/macos/rustafari.icns missing — bundling without an icon"
fi

if [[ -n "${MACOS_SIGN_IDENTITY:-}" ]]; then
	echo "==> Signing"
	codesign --force --deep --options runtime --timestamp \
		--sign "$MACOS_SIGN_IDENTITY" "$APP"
	codesign --verify --strict --verbose=2 "$APP"
else
	echo "!! MACOS_SIGN_IDENTITY unset — producing an UNSIGNED bundle"
fi

echo "==> Building $DMG"
hdiutil create -volname rustafari -srcfolder "$APP" -ov -format UDZO "$DMG" >/dev/null

if [[ -n "${MACOS_NOTARY_PROFILE:-}" ]]; then
	echo "==> Notarizing (this waits on Apple, typically a few minutes)"
	xcrun notarytool submit "$DMG" --keychain-profile "$MACOS_NOTARY_PROFILE" --wait
	xcrun stapler staple "$DMG"
fi

echo "==> Done: $DMG"
shasum -a 256 "$DMG"
