#!/bin/sh
# Captures the Kobo's framebuffer into a PNG: scripts/kobo-screenshot.sh out.png
set -eu
OUT=${1:-kobo.png}
RAW=$(mktemp)
trap 'rm -f "$RAW"' EXIT
GEOMETRY=$(echo 'cat /sys/class/graphics/fb0/virtual_size; cat /sys/class/graphics/fb0/stride' | "$(dirname "$0")/kobo.sh")
echo 'cat /dev/fb0' | "$(dirname "$0")/kobo.sh" > "$RAW"
python3 -I - "$RAW" "$OUT" $GEOMETRY <<'PY'
import struct, sys, zlib
raw_path, out, size, stride = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
width, height = map(int, size.split(","))
data = open(raw_path, "rb").read()
rows = []
for y in range(height):
    line = data[y * stride : y * stride + width * 4]
    rgb = bytearray(width * 3)
    rgb[0::3], rgb[1::3], rgb[2::3] = line[2::4], line[1::4], line[0::4]
    rows.append(b"\0" + bytes(rgb))
def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"".join(rows), 6)) + chunk(b"IEND", b"")
open(out, "wb").write(png)
PY
echo "$OUT"
