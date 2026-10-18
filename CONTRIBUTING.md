# Contributing

Issues and pull requests are welcome. Reports from Kobo models other than the Clara Colour are
especially useful: say which model and firmware, and what worked or didn't.

## Run the daemon on your computer

```sh
cargo test --workspace
rm -f fixtures/KoboReader.sqlite
cat fixtures/schema-legacy.sql fixtures/schema-migrations.sql fixtures/sample-data.sql | sqlite3 fixtures/KoboReader.sqlite
cargo run -p reliod -- --db fixtures/KoboReader.sqlite --onboard fixtures --state /tmp/relio --listen 127.0.0.1:8080
```

Open http://127.0.0.1:8080, then run `cargo run -p reliod -- pair` in another terminal (it stands
in for the Kobo's menu) and type the code in the browser.

## Build for the Kobo

```sh
brew install zig && cargo install cargo-zigbuild
rustup target add armv7-unknown-linux-musleabihf
scripts/package-kobo.sh
```

`package-kobo.sh` builds the static daemon and the plugin (in the NickelTC Docker image, so Docker
must be running) and writes `dist/KoboRoot.tgz`.

It also writes the installer page to `dist/site`, with the package next to it. The page is static,
so any web server works:

```sh
python3 -m http.server 8787 --directory dist/site
```

## Work on a device

Enable SSH in the firmware (`.kobo/ssh-enabled`, set a root password, install your key), then put
the e-reader's address in `scripts/kobo.env`, which git ignores:

```sh
KOBO_HOST=192.168.1.20
KOBO_KEY="$HOME/.ssh/kobo"
```

The `scripts/kobo*.sh` helpers run commands over SSH, push files, deploy the plugin and grab
screenshots from the framebuffer. [docs/architecture.md](docs/architecture.md) lists them.

## Translations

Every user-facing text lives in `i18n/<language>.json`: `kobo` for the e-reader panel, `web` for
the dashboard and `install` for the installer page. `en.json` is the reference. To add a language,
copy it under the language's BCP 47 tag, translate the values and run:

```sh
node scripts/check-i18n.mjs
```

It checks keys, placeholders and CLDR plural forms. The daemon and the plugin pick up the new file
at build time.

## Releasing

1. Set the new `version` in the workspace `Cargo.toml` and commit.
2. Tag the commit `v<version>` and push the tag.

The Release workflow then builds `KoboRoot.tgz`, writes `update.json` with
`scripts/release-manifest.py`, signs it, publishes the GitHub release and deploys the installer
page. E-readers find the new version through that manifest within a day.

One-time setup of the repository:

- The `MINISIGN_KEY` secret holds the minisign secret key whose public half is `RELEASE_KEY` in
  `crates/reliod/src/update.rs`. The workflow checks the signature against that key before
  publishing anything.
- GitHub Pages uses GitHub Actions as its source, with the custom domain relio.androne.dev (a
  CNAME record to the owner's `github.io` domain).
- The `github-pages` environment only accepts the default branch at first. Add a deployment rule
  for `v*` tags, or the release stops before deploying the page.

If the project moves, change `MANIFEST_URLS` in `update.rs` and `SITE_URL` in `release.yml`, and
keep the old address answering until most e-readers have installed a release that knows the new
one. A new signing key works the same way: the last release signed with the old key ships the new
public key.

## Licensing

Contributions are accepted under the [Apache License 2.0](LICENSE), like the rest of the project.
