#!/bin/sh
# Runs shell commands from stdin on the Kobo (its sshd ignores remote command arguments).
# KOBO_HOST, and KOBO_KEY for a dedicated SSH key, come from the environment or scripts/kobo.env.
set -eu
ENV_FILE="$(dirname "$0")/kobo.env"
if [ -f "$ENV_FILE" ]; then . "$ENV_FILE"; fi
: "${KOBO_HOST:?set KOBO_HOST to the Kobo address, for example in scripts/kobo.env}"
exec ssh -T -o BatchMode=yes -o ConnectTimeout=8 ${KOBO_KEY:+-i "$KOBO_KEY"} "root@$KOBO_HOST"
