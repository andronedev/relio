#!/bin/sh
# Copies a local file to the Kobo atomically: scripts/kobo-push.sh local remote
set -eu
LOCAL=$1
REMOTE=$2
{
    echo "base64 -d > '$REMOTE.tmp' <<'RELIO_EOF'"
    base64 < "$LOCAL"
    echo "RELIO_EOF"
    echo "chmod $(stat -f '%Lp' "$LOCAL") '$REMOTE.tmp' && mv '$REMOTE.tmp' '$REMOTE' && md5sum '$REMOTE'"
} | "$(dirname "$0")/kobo.sh"
md5 -r "$LOCAL"
