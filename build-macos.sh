#!/bin/sh
# Builds "VS Recent.app" as a universal (Apple silicon and Intel) application and zips it.
set -eu
cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust is required. Install it from https://rustup.rs" >&2
    exit 1
fi

export MACOSX_DEPLOYMENT_TARGET=11.0
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
app="publish/VS Recent.app"
archive="publish/vsrecent-macos-universal.zip"

if command -v rustup >/dev/null 2>&1; then
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
fi
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin

rm -rf "$app" "$archive"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create \
    target/aarch64-apple-darwin/release/vsrecent \
    target/x86_64-apple-darwin/release/vsrecent \
    -output "$app/Contents/MacOS/vsrecent"
sed -e "s/__VERSION__/$version/g" \
    -e "s/__MINIMUM_MACOS__/$MACOSX_DEPLOYMENT_TARGET/g" \
    macos/Info.plist > "$app/Contents/Info.plist"
cp macos/vsrecent.icns "$app/Contents/Resources/vsrecent.icns"

# An ad-hoc signature lets the app run on Apple silicon. It is not a Developer ID signature, so
# Gatekeeper still asks before the first launch of a downloaded copy.
codesign --force --sign - "$app"
ditto -c -k --keepParent "$app" "$archive"
echo "Built $app and $archive"
