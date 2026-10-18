# Firmware notes

The test device runs 4.46.23836 (August 2026); its Qt and boot scripts match 4.44 for everything
Relio relies on.

Findings from `kobo-update-4.44.23552.zip` for the Clara Colour (`kobo12`, model id `…393`),
fetched from Kobo's CDN via the index at pgaskin.net/KoboStuff/kobofirmware.html.

## Hardware and userland

| Item | Value |
| --- | --- |
| SoC | MediaTek MT8113T (`upgrade/mt8113t-ntx`) |
| Userland | 32-bit ARM EABI5 hard-float, glibc 2.11.1 |
| UI | Nickel, Qt 5.2.1 (`/usr/local/Qt-5.2.1-arm`) |
| Init | busybox `inittab` → `/etc/init.d/rcS` → `/usr/local/Kobo/nickel -platform kobo` |
| Shared with | Clara BW (`kobo12`, `…391`) uses the same update package |

glibc 2.11 is too old to target directly, so binaries are built static against musl
(`armv7-unknown-linux-musleabihf`). Every Kobo since the Touch is ARMv7, so one binary fits all.
Older devices with Linux 2.6.35 kernels sit below Rust's official minimum (3.2) and need testing on hardware.

## Install and boot hooks

- `rcS` extracts `/mnt/onboard/.kobo/KoboRoot.tgz` over `/` on boot, then deletes it.
- udev rules live in `/lib/udev/rules.d/`; a rule file shipped in `KoboRoot.tgz` can start the daemon
  without any third-party launcher.
- Nickel loads Qt plugins from `/usr/local/Kobo/imageformats/`, which is the injection point for the
  status-bar button (our own plugin, no NickelMenu/NickelHook).

## Status bar

`libnickel.so.1.0.0` exports the classes needed to add an icon next to search, Wi-Fi and sync:
`StatusBarView` (ctor `_ZN13StatusBarViewC1EP7QWidget`, slots `search()`, `sync()`, `wifi()`),
`StatusBarController`, and `StatusBarIcon` (`setAnimatedSvgPath`, `setSelectedState`) as the
base class of the built-in icons (`StatusBarSyncIcon`, `StatusBarWiFiIcon`, …).

## Database (`.kobo/KoboReader.sqlite`)

Nickel creates a base schema and grows it with `ALTER TABLE … ADD COLUMN` migrations, so the
columns present depend on the firmware that last opened the database. `relio-core` resolves
columns at runtime; `fixtures/schema-legacy.sql` is the base schema and
`fixtures/schema-migrations.sql` the migrations Relio reads.

| Table | Used for |
| --- | --- |
| `content` (`ContentType = 6`) | Books: title, author, `ReadStatus` (0/1/2), `___PercentRead`, `DateLastRead`, `TimeSpentReading`, `RestOfBookEstimate` |
| `content` (`ContentType = 9`, `899`) | Chapters of a book |
| `Bookmark` | Highlights, notes, dog-ears; `Type` ∈ `highlight`, `note`, `dogear`, `markup`; `Color` on colour devices; `Hidden` for deleted |
| `Shelf`, `ShelfContent` | Collections, soft-deleted with `_IsDeleted` |

Sideloaded books have `ContentID = file:///<path>` (usually `/mnt/onboard/…`); store books use a UUID.
Booleans such as `IsDownloaded` and `Hidden` are stored as the text `true`/`false` on current
firmware, and `ContentType` as text.
`HighlightColor` names in libnickel are Yellow, Pink, Blue, Green, Gray, but the Clara Colour renders index 0 as cyan; its picker shows cyan, purple, yellow, green, assumed to be indices 0–3 (only 0 verified).

## Runtime facts (Clara Colour, 4.46)

- Framebuffer `hwtcon`: 1072×1448, 32 bpp BGRA, stride 4288.
- Touchscreen `cyttsp5_mt` on `/dev/input/event1`, multitouch protocol A (no slots),
  `ABS_MT_POSITION_X` 0–1447 along the screen height, `ABS_MT_POSITION_Y` 0–1071 mirrored along the width.
- The loopback interface `lo` is down.
- `KoboReader.sqlite` runs in WAL mode: recent writes live in `-wal`, so copying the main file alone misses them.
- Nickel keeps the open book's annotations in memory; reliod refuses annotation edits while `/tmp/relio-reading` exists.
- `/etc/init.d/ssh` starts `sshd` when `.kobo/ssh-enabled` exists; until a root password is set it
  allows empty-password root logins, and it forces `passwd` on the first login.
- The status bar is `StatusBarView` → `statusbarContainer` (time label + `iconContainer` row of
  `StatusBarIcon`s ending with `searchIcon`), 107 px tall; icon glyphs are 42 px, centred 72 px down.
