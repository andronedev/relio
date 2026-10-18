#!/bin/sh
# Started by udev when Wi-Fi comes up; detaches so udev does not reap it.
DIR=/usr/local/relio
STATE=/mnt/onboard/.adds/relio

start() {
    setsid "$DIR/reliod" --onboard /mnt/onboard >>/tmp/reliod.log 2>&1 </dev/null &
}

# reliod.prev is kept until a newly installed version has run for a minute. If the new one
# stops right away, the previous one comes back and the daemon skips that version.
if [ "$1" = "watch" ]; then
    sleep 20
    pidof reliod >/dev/null && exit 0
    mv "$DIR/reliod.prev" "$DIR/reliod" || exit 1
    mkdir -p "$STATE" && touch "$STATE/rolled-back"
    start
    exit 0
fi

[ "$STARTUP" = "1" ] && exit 0
pidof reliod >/dev/null && exit 0
start
if [ -x "$DIR/reliod.prev" ]; then
    setsid "$0" watch </dev/null >/dev/null 2>&1 &
fi
