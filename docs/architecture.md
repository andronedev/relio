# Architecture

```
 Kobo (any model)                                      Phone / computer
┌───────────────────────────────┐                    ┌──────────────────┐
│ Nickel                        │                    │ browser          │
│  └ Relio Qt plugin ───────────┼─ button ─┐         │  └ dashboard     │
│ reliod (static musl)          │◀─────────┘         │    (PWA)         │
│  ├ relio-core   (SQLite)      │                    └────────▲─────────┘
│  ├ relio-kepub  (EPUB→KEPUB)  │                             │
│  └ HTTP API + dashboard       │◀──────── local Wi-Fi ───────┘
└───────────────────────────────┘
```

## Crates

| Crate | Role |
| --- | --- |
| `relio-core` | Reads and edits `KoboReader.sqlite`, schema-tolerant across firmware versions |
| `relio-kepub` | EPUB to KEPUB conversion, a port of kepubify |
| `reliod` | Daemon on the e-reader: pairing, local HTTP API and the embedded dashboard |
| `kobo-touch` | Development tool: inspects the touchscreen and injects taps |

## Pairing and privacy

- Everything stays on the local network: the dashboard is served by the e-reader and nothing is
  sent anywhere else.
- Browsers are paired from the e-reader: the status-bar menu opens a 6-digit code, valid 3 minutes
  and 5 attempts, shown with a QR code of `http://<ip>:8080/#code=…`; the browser exchanges it for its
  own token. Only token hashes are kept on the device (`.adds/relio/clients.json`), and each
  browser can be revoked, from the dashboard API or from the e-reader's Connected devices page.
  Codes can only be opened through the control socket, so only the device UI can start a pairing.
- The dashboard makes no third-party requests (system fonts, no CDN, `no-referrer`).

## Status-bar plugin (`nickel/`)

`librelio.so` is installed in `/usr/local/Kobo/imageformats`, where Qt dlopens every library, so
its constructor runs inside Nickel. It patches nothing: an application-wide event filter waits for a
`StatusBarView` to be shown and inserts the Relio icon before Nickel's first status-bar icon. The
icon opens a full-window overlay that asks `reliod` for a pairing code, QR code and dashboard URL
over the control socket `/tmp/reliod.sock` (the Kobo keeps its loopback interface down, so
`127.0.0.1` is unusable). Nickel delivers touch events without synthesizing mouse clicks, so the icon
and the panel buttons handle `TouchBegin`/`TouchEnd` themselves.

- Failsafe: the library renames itself to `.failsafe` while loading and is restored after 20 s, so a
  crash during boot disables it on the next start.
- Kill switch: an empty `.adds/relio/disable-plugin` file keeps it inactive.
- Diagnostics: `.adds/relio/plugin.log`, including a dump of the status bar's widget tree.
- Qt only dlopens libraries that carry plugin metadata, so `image_plugin.h` declares a no-op image
  plugin with a unique key.
- The icon goes into Nickel's `iconContainer` row before `searchIcon`, and copies the search glyph's
  measured size and position.
- Built with the NickelTC Docker image (GCC 4.9, Qt 5.2.1) by `scripts/build-nickel-plugin.sh`; it
  only imports baseline glibc/libstdc++ symbols and Qt symbols present in the firmware's Qt.

## Device development loop

With the firmware's SSH enabled (`.kobo/ssh-enabled`, root password set, key installed), and
`KOBO_HOST` (plus `KOBO_KEY` if needed) set in the environment or in `scripts/kobo.env`:

| Script | Does |
| --- | --- |
| `scripts/kobo.sh` | Runs shell commands from stdin on the Kobo (its sshd ignores command arguments) |
| `scripts/kobo-push.sh local remote` | Copies a file through base64 (scp hangs on the device) |
| `scripts/kobo-screenshot.sh out.png` | Converts `/dev/fb0` to PNG |
| `scripts/kobo-deploy-plugin.sh` | Builds and installs the plugin, reboots, waits for SSH |
| `kobo-touch tap X Y` | Injects a tap; touch X = screen Y, touch Y = 1071 − screen X on the Clara Colour |

## Books and covers

- `POST /api/books?name=…&kepub=true` streams an upload to the root of the user partition (never overwriting), converts
  EPUB to KEPUB with `relio-kepub` when asked, then drops `/tmp/relio-rescan`.
- `DELETE /api/books?id=…` removes a sideloaded file, or a purchase's download in `.kobo/kepub/`
  (the purchase stays in the Kobo account); the dashboard asks for confirmation first.
- The plugin watches `/tmp` and calls `PlugWorkflowManager::sync()`, Nickel's own import, which adds
  or drops the books without leaving the current screen.
- Covers come from Nickel's cache, `.kobo-images/<a>/<b>/<ImageId> - N3_LIBRARY_{GRID,FULL}.parsed`
  (JPEG), indexed by `reliod` and re-indexed when one is missing.

## Annotations

`PATCH /api/annotations/{id}` changes a note or colour and `DELETE` removes one, each in a short
`BEGIN IMMEDIATE` transaction that only touches columns the firmware has, stamping `DateModified` in
Nickel's format. Annotations never synced to the Kobo cloud are deleted; synced ones are marked
`Hidden` so the deletion syncs. Both are refused (409) while the plugin's `/tmp/relio-reading`
flag says a book is open, because Nickel keeps that book's annotations in memory.

## Languages

`i18n/*.json` holds every text in 30 languages. The dashboard and the installer pick the browser's
language (or the one chosen in their footer), format numbers, durations and dates with `Intl`, and
switch to right-to-left layout for Arabic and Hebrew through CSS logical properties. The plugin
follows `ApplicationPreferences/CurrentLocale` in `Kobo eReader.conf` and is rebuilt on every
opening so a language change on the e-reader applies immediately.

## Dashboard as an app

The dashboard ships a web manifest, icons and a service worker that caches the app shell (never
`/api`). Browsers only enable service workers in secure contexts, so over the Kobo's plain-HTTP LAN
address it installs as a home-screen shortcut rather than an offline app.

## Reliability rules

- The library is opened read-only, one connection per request, so Nickel can always write and checkpoint.
- Writes are short transactions on the columns the firmware has, and wait until no book is open.
- The Kobo turns Wi-Fi off when idle, which stops the daemon's network: the dashboard then shows the
  e-reader as unreachable until Wi-Fi comes back.

## Updates (`crates/reliod/src/update.rs`)

- Only the copy installed in `/usr/local/relio` updates itself; development builds never do.
- A minute after it starts, then about once a day of wall-clock time, the daemon reads
  `update.json` from relio.androne.dev, or from the latest GitHub release if that fails. The
  e-reader's sleep pauses the monotonic clock, so the loop wakes every ten minutes and compares
  wall-clock times.
- The manifest lists the version, size, SHA-256 and download addresses of `KoboRoot.tgz`. It is
  only trusted if `update.json.minisig` verifies against the release key compiled into the daemon,
  and only acted on if its version is newer, so an old signed release cannot be replayed.
- The package downloads to `.kobo/relio-update.part`, is checked against the size and hash, and is
  renamed to `.kobo/KoboRoot.tgz`, which the e-reader installs while restarting. Nothing is staged
  while another `KoboRoot.tgz` (a Kobo firmware update) is waiting, or while the battery is under
  20 % and discharging. `.adds/relio/no-updates` turns the checks off.
- Before staging, the daemon copies itself to `reliod.prev`. After a restart, `launch.sh` waits 20
  seconds; if the new daemon is not running, it puts `reliod.prev` back and leaves
  `.adds/relio/rolled-back`, so the daemon skips that version. Once a version has run for a minute,
  `reliod.prev` is removed.
- The plugin reads the state from `GET /update` on the control socket. When a package is ready, the
  panel offers to restart through `N3PowerWorkflowManager::reboot`, Nickel's own restart, with
  `POST /update/restart` (`sync; reboot`) as the fallback.

## Install flow

1. The user opens the installer site in a desktop browser and plugs in the Kobo.
2. The site asks for the `KOBOeReader` volume with the File System Access API
   (`showDirectoryPicker`, Chrome and Edge) and writes `.kobo/KoboRoot.tgz`. WebUSB cannot be used:
   browsers block the USB mass-storage class. Safari and Firefox get a download and a short
   "drop it into `.kobo`" step instead.
3. After ejecting, the Kobo installs the package on reboot: `reliod`, its udev start rule and the Qt plugin.
4. The Relio button in the status bar shows a pairing code, a QR code and the dashboard address.

`KoboRoot.tgz` is built by `scripts/package-kobo.sh` from `packaging/kobo/`. On MediaTek devices udev
is not re-triggered at boot (`rcS` restores a cached `/dev`), so the daemon is started by a udev rule
on `wlan*` appearing, which is exactly when it is needed; `launch.sh` detaches it with `setsid`.
