#!/bin/sh
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
python3 "$ROOT/scripts/gen-kobo-strings.py" "$ROOT/nickel/build/strings_table.inc"
docker run --rm --platform linux/amd64 -v "$ROOT/nickel:/src" -w /src ghcr.io/pgaskin/nickeltc:1.0 make
echo "$ROOT/nickel/build/librelio.so"
