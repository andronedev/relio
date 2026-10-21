#!/bin/sh
# Assembles the installer site in dist/site: the page, translations, icon, and the package if built.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT="$ROOT/dist/site"

rm -rf "$OUT"
mkdir -p "$ROOT/dist"
cp -R "$ROOT/web/site" "$OUT"
cp "$ROOT/web/i18n.js" "$ROOT/web/favicon.svg" "$OUT/"
if [ -f "$ROOT/dist/KoboRoot.tgz" ]; then
    cp "$ROOT/dist/KoboRoot.tgz" "$OUT/"
fi
mkdir -p "$OUT/i18n"
cp "$ROOT"/i18n/*.json "$OUT/i18n/"
python3 -c 'import json, pathlib, sys
files = sorted(pathlib.Path(sys.argv[1]).glob("*.json"))
print(json.dumps({f.stem: json.loads(f.read_text("utf-8"))["meta"] for f in files}, ensure_ascii=False))' "$ROOT/i18n" > "$OUT/i18n/index.json"
echo "$OUT"
