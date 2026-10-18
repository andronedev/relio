#!/bin/sh
set -eu

TARGET=armv7-unknown-linux-musleabihf
ROOT=$(cd "$(dirname "$0")/.." && pwd)
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

cargo zigbuild --release -p reliod --target "$TARGET" --manifest-path "$ROOT/Cargo.toml"
"$ROOT/scripts/build-nickel-plugin.sh"

cp -R "$ROOT/packaging/kobo/." "$STAGE/"
install -m 755 "$ROOT/target/$TARGET/release/reliod" "$STAGE/usr/local/relio/reliod"
install -d "$STAGE/usr/local/Kobo/imageformats"
install -m 644 "$ROOT/nickel/build/librelio.so" "$STAGE/usr/local/Kobo/imageformats/librelio.so"
chmod 644 "$STAGE/etc/udev/rules.d/99-relio.rules"

mkdir -p "$ROOT/dist"
# GNU tar (CI) and bsdtar (macOS) spell the root owner differently.
if tar --version 2>/dev/null | grep -q GNU; then
    OWNER="--owner=0 --group=0 --numeric-owner"
else
    OWNER="--uid 0 --gid 0"
fi
COPYFILE_DISABLE=1 tar --format ustar --no-xattrs $OWNER -czf "$ROOT/dist/KoboRoot.tgz" -C "$STAGE" etc usr
echo "$ROOT/dist/KoboRoot.tgz"
"$ROOT/scripts/build-site.sh"
