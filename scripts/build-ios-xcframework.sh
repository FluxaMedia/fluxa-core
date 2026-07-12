#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname)" != "Darwin" ]]; then
    echo "error: this script requires macOS with Xcode command line tools installed" >&2
    exit 1
fi

if ! command -v xcodebuild >/dev/null 2>&1; then
    echo "error: xcodebuild not found — install Xcode" >&2
    exit 1
fi

cd "$(dirname "$0")/.."

TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim)

for target in "${TARGETS[@]}"; do
    rustup target add "$target"
done

for target in "${TARGETS[@]}"; do
    cargo build --release --target "$target" --no-default-features --features ios --lib
done

BINDINGS_DIR="target/uniffi-swift"
rm -rf "$BINDINGS_DIR"
mkdir -p "$BINDINGS_DIR"

cargo run --features uniffi-cli --bin uniffi-bindgen generate \
    --library "target/aarch64-apple-ios/release/libfluxa_core.a" \
    --language swift \
    --config uniffi.toml \
    --out-dir "$BINDINGS_DIR"

HEADERS_DIR="$BINDINGS_DIR/Headers"
mkdir -p "$HEADERS_DIR"
mv "$BINDINGS_DIR"/*.h "$HEADERS_DIR"/
mv "$BINDINGS_DIR"/*.modulemap "$HEADERS_DIR/module.modulemap"

XCFRAMEWORK_OUT="build/ios/FluxaCore.xcframework"
rm -rf "build/ios"
mkdir -p "build/ios"

xcodebuild -create-xcframework \
    -library "target/aarch64-apple-ios/release/libfluxa_core.a" -headers "$HEADERS_DIR" \
    -library "target/aarch64-apple-ios-sim/release/libfluxa_core.a" -headers "$HEADERS_DIR" \
    -output "$XCFRAMEWORK_OUT"

echo "xcframework: $XCFRAMEWORK_OUT"
echo "Swift bindings: $BINDINGS_DIR"/*.swift
