#!/bin/sh
# Builds the Nickel plugin, installs it on the Kobo, reboots and waits until it is reachable again.
set -eu
DIR=$(dirname "$0")
"$DIR/build-nickel-plugin.sh" >/dev/null
"$DIR/kobo-push.sh" "$DIR/../nickel/build/librelio.so" /usr/local/Kobo/imageformats/librelio.so
echo 'sync; reboot' | "$DIR/kobo.sh"
sleep 20
until echo true | "$DIR/kobo.sh" >/dev/null 2>&1; do sleep 3; done
sleep 10
echo "Kobo is back"
