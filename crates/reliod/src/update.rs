//! Keeps Relio up to date. While Wi-Fi is on, the daemon reads the release manifest about once
//! a day, checks its signature against the release key, then downloads the package it lists
//! and checks its size and SHA-256. The package is left as `.kobo/KoboRoot.tgz`, which the
//! e-reader installs the next time it restarts.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::token::to_hex;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The project's own domain comes first so that devices keep finding updates if the repository
/// moves; the second address reads the latest GitHub release directly.
const MANIFEST_URLS: &[&str] = &[
    "https://relio.androne.dev/update.json",
    "https://github.com/andronedev/relio/releases/latest/download/update.json",
];

/// Public half of the minisign key that signs every release manifest.
const RELEASE_KEY: &str = "RWRUcObpQyo/2pHGKuUqfgGiZpx4jb6eVK2IPG4oEKBgdgEB6WpMVisG";

/// Updates only run for the copy installed on the e-reader, never for a development build.
const INSTALL_DIR: &str = "/usr/local/relio";

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);
/// Sleep is measured on a clock that stops while the e-reader sleeps, so the loop wakes often
/// and compares wall-clock times instead.
const TICK: Duration = Duration::from_secs(600);
const CHECK_EVERY: u64 = 24 * 3600;
const RETRY_AFTER: u64 = 6 * 3600;
const LOW_BATTERY_RETRY: u64 = 3600;
const MIN_BATTERY: u32 = 20;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    Idle,
    Checking,
    Downloading { version: String },
    Ready { version: String },
    Disabled,
}

/// How the last check ended, for a person who asked for it from the panel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    UpToDate,
    Blocked,
    Failed,
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub size: u64,
    pub sha256: String,
    pub urls: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    last_check: u64,
    staged: Option<String>,
    skipped: Option<String>,
}

struct Paths {
    package: PathBuf,
    partial: PathBuf,
    saved: PathBuf,
    opt_out: PathBuf,
    rolled_back: PathBuf,
    ready_flag: PathBuf,
    /// The running binary and the copy `launch.sh` restores if a new version fails to start.
    backup: Option<(PathBuf, PathBuf)>,
}

pub struct Updater {
    paths: Paths,
    status: Mutex<Status>,
    last: Mutex<Option<Outcome>>,
    requested: Mutex<bool>,
    wake: Condvar,
    enabled: bool,
}

impl Updater {
    pub fn new(onboard: &Path, state_dir: &Path, ready_flag: PathBuf) -> Self {
        let installed = std::env::current_exe().ok().filter(|exe| exe.starts_with(INSTALL_DIR));
        let paths = Paths {
            package: onboard.join(".kobo/KoboRoot.tgz"),
            partial: onboard.join(".kobo/relio-update.part"),
            saved: state_dir.join("update.json"),
            opt_out: state_dir.join("no-updates"),
            rolled_back: state_dir.join("rolled-back"),
            ready_flag,
            backup: installed.as_ref().map(|exe| (exe.clone(), exe.with_file_name("reliod.prev"))),
        };
        Self::with_paths(paths, installed.is_some())
    }

    fn with_paths(paths: Paths, enabled: bool) -> Self {
        Self {
            paths,
            status: Mutex::new(Status::Idle),
            last: Mutex::new(None),
            requested: Mutex::new(false),
            wake: Condvar::new(),
            enabled,
        }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn last_outcome(&self) -> Option<Outcome> {
        *self.last.lock().unwrap()
    }

    /// Checks right away, from the panel. Returns false when this build never updates itself.
    pub fn request_check(&self) -> bool {
        if !self.enabled {
            return false;
        }
        if matches!(self.status(), Status::Idle | Status::Disabled) {
            self.set(Status::Checking);
        }
        *self.requested.lock().unwrap() = true;
        self.wake.notify_all();
        true
    }

    pub fn spawn(self: std::sync::Arc<Self>) {
        if !self.enabled {
            self.set(Status::Disabled);
            return;
        }
        std::thread::spawn(move || self.run());
    }

    fn run(&self) {
        self.wait(FIRST_CHECK_DELAY);
        let key = PublicKey::from_base64(RELEASE_KEY).expect("release key");
        let agent = agent(true);
        let mut saved = self.reconcile();
        let mut next_attempt = saved.last_check + CHECK_EVERY;
        loop {
            let manual = std::mem::take(&mut *self.requested.lock().unwrap());
            if manual || now() >= next_attempt {
                next_attempt = now() + self.attempt(&agent, &key, &mut saved, manual);
            }
            self.wait(TICK);
        }
    }

    /// Sleeps until the timeout or a check requested from the panel, whichever comes first.
    fn wait(&self, timeout: Duration) {
        let requested = self.requested.lock().unwrap();
        if !*requested {
            let _ = self.wake.wait_timeout(requested, timeout).unwrap();
        }
    }

    /// Settles what happened to a staged package while the e-reader restarted.
    fn reconcile(&self) -> Saved {
        let mut saved = self.load();
        let _ = fs::remove_file(&self.paths.partial);
        if let Some(version) = saved.staged.clone() {
            if version == VERSION {
                println!("update: version {VERSION} installed");
                saved.staged = None;
            } else if self.paths.rolled_back.exists() && !self.paths.package.exists() {
                eprintln!("update: version {version} did not start, back on {VERSION}");
                saved.skipped = Some(version);
                saved.staged = None;
            } else if self.paths.package.exists() {
                self.set(Status::Ready { version });
            } else {
                saved.staged = None;
            }
        }
        // The running version has started fine, so the copy kept for a rollback is no longer
        // needed, unless it backs a package still waiting to be installed.
        if !matches!(self.status(), Status::Ready { .. }) {
            if let Some((_, previous)) = &self.paths.backup {
                let _ = fs::remove_file(previous);
            }
        }
        let _ = fs::remove_file(&self.paths.rolled_back);
        self.save(&saved);
        saved
    }

    /// Returns how many seconds to wait before the next attempt. A manual check also runs when
    /// automatic checks are turned off, and does not wait for the battery.
    fn attempt(&self, agent: &ureq::Agent, key: &PublicKey, saved: &mut Saved, manual: bool) -> u64 {
        if !manual && self.paths.opt_out.exists() {
            self.set(Status::Disabled);
            return CHECK_EVERY;
        }
        if matches!(self.status(), Status::Ready { .. }) {
            return CHECK_EVERY;
        }
        self.set(Status::Checking);
        let result = self.update(agent, key, saved, manual);
        if !matches!(self.status(), Status::Ready { .. }) {
            self.set(Status::Idle);
        }
        let (wait, outcome) = result.unwrap_or_else(|err| {
            eprintln!("update: {err}");
            let _ = fs::remove_file(&self.paths.partial);
            (RETRY_AFTER, Some(Outcome::Failed))
        });
        *self.last.lock().unwrap() = outcome;
        wait
    }

    fn update(
        &self,
        agent: &ureq::Agent,
        key: &PublicKey,
        saved: &mut Saved,
        manual: bool,
    ) -> Result<(u64, Option<Outcome>), String> {
        let manifest = fetch_manifest(agent, MANIFEST_URLS, key)?;
        let wanted = is_newer(&manifest.version, VERSION) && saved.skipped.as_deref() != Some(&manifest.version);
        if !wanted {
            saved.last_check = now();
            self.save(saved);
            return Ok((CHECK_EVERY, Some(Outcome::UpToDate)));
        }
        if self.paths.package.exists() {
            println!("update: {} waits for the KoboRoot.tgz already pending", manifest.version);
            return Ok((RETRY_AFTER, Some(Outcome::Blocked)));
        }
        if !manual && battery_low() {
            println!("update: {} waits for the battery to charge", manifest.version);
            return Ok((LOW_BATTERY_RETRY, None));
        }

        println!("update: downloading {}", manifest.version);
        self.set(Status::Downloading { version: manifest.version.clone() });
        self.download(agent, &manifest)?;
        self.stage()?;
        saved.last_check = now();
        saved.staged = Some(manifest.version.clone());
        self.save(saved);
        println!("update: {} ready, installs when the e-reader restarts", manifest.version);
        self.set(Status::Ready { version: manifest.version });
        Ok((CHECK_EVERY, None))
    }

    fn download(&self, agent: &ureq::Agent, manifest: &Manifest) -> Result<(), String> {
        let mut errors = vec![String::from("no package address")];
        for url in &manifest.urls {
            // ureq's own errors already name the address.
            let attempt = agent.get(url).call().map_err(|err| err.to_string()).and_then(|response| {
                File::create(&self.paths.partial)
                    .map_err(|err| err.to_string())
                    .and_then(|file| write_verified(response.into_reader(), file, manifest))
                    .map_err(|err| format!("{url}: {err}"))
            });
            match attempt {
                Ok(()) => return Ok(()),
                Err(err) => errors.push(err),
            }
        }
        Err(errors[1..].join("; "))
    }

    fn stage(&self) -> Result<(), String> {
        if let Some((current, previous)) = &self.paths.backup {
            fs::copy(current, previous).map_err(|err| format!("cannot keep the current version: {err}"))?;
        }
        fs::rename(&self.paths.partial, &self.paths.package).map_err(|err| format!("cannot stage the package: {err}"))?;
        if let Some(dir) = self.paths.package.parent() {
            let _ = File::open(dir).and_then(|dir| dir.sync_all());
        }
        Ok(())
    }

    fn set(&self, status: Status) {
        let flag = match &status {
            Status::Ready { version } => fs::write(&self.paths.ready_flag, version),
            _ => fs::remove_file(&self.paths.ready_flag).or_else(|err| match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(err),
            }),
        };
        if let Err(err) = flag {
            eprintln!("update: cannot update {}: {err}", self.paths.ready_flag.display());
        }
        *self.status.lock().unwrap() = status;
    }

    fn load(&self) -> Saved {
        fs::read(&self.paths.saved).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    fn save(&self, saved: &Saved) {
        let tmp = self.paths.saved.with_extension("tmp");
        let written = serde_json::to_vec(saved)
            .map_err(std::io::Error::other)
            .and_then(|bytes| fs::write(&tmp, bytes))
            .and_then(|()| fs::rename(&tmp, &self.paths.saved));
        if let Err(err) = written {
            eprintln!("update: cannot save {}: {err}", self.paths.saved.display());
        }
    }
}

fn agent(https_only: bool) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .https_only(https_only)
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .user_agent(&format!("relio/{VERSION}"))
        .build()
}

fn fetch_manifest(agent: &ureq::Agent, urls: &[&str], key: &PublicKey) -> Result<Manifest, String> {
    let mut errors = Vec::new();
    for url in urls {
        let attempt = fetch_text(agent, url)
            .and_then(|body| fetch_text(agent, &format!("{url}.minisig")).map(|signature| (body, signature)))
            .and_then(|(body, signature)| {
                parse_manifest(body.as_bytes(), &signature, key).map_err(|err| format!("{url}: {err}"))
            });
        match attempt {
            Ok(manifest) => return Ok(manifest),
            Err(err) => errors.push(err),
        }
    }
    Err(errors.join("; "))
}

/// ureq's own errors already name the address; read errors get it added.
fn fetch_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let mut text = String::new();
    agent
        .get(url)
        .call()
        .map_err(|err| err.to_string())?
        .into_reader()
        .take(MAX_MANIFEST_BYTES)
        .read_to_string(&mut text)
        .map_err(|err| format!("{url}: {err}"))?;
    Ok(text)
}

pub fn parse_manifest(body: &[u8], signature: &str, key: &PublicKey) -> Result<Manifest, String> {
    let signature = Signature::decode(signature).map_err(|err| format!("unreadable signature: {err}"))?;
    key.verify(body, &signature, false).map_err(|err| format!("signature rejected: {err}"))?;
    let manifest: Manifest = serde_json::from_slice(body).map_err(|err| format!("unreadable manifest: {err}"))?;
    let valid = parse_version(&manifest.version).is_some()
        && manifest.size <= MAX_PACKAGE_BYTES
        && manifest.sha256.len() == 64
        && manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        && manifest.urls.iter().all(|url| url.starts_with("https://"));
    if valid { Ok(manifest) } else { Err("manifest out of range".to_owned()) }
}

/// Copies at most one byte more than announced, so an oversized download is caught.
fn write_verified(reader: impl Read, mut file: File, manifest: &Manifest) -> Result<(), String> {
    let mut reader = reader.take(manifest.size + 1);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = reader.read(&mut buffer).map_err(|err| err.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|err| err.to_string())?;
        total += read as u64;
    }
    file.sync_all().map_err(|err| err.to_string())?;
    if total != manifest.size {
        return Err(format!("received {total} bytes instead of {}", manifest.size));
    }
    if to_hex(&hasher.finalize()) != manifest.sha256.to_ascii_lowercase() {
        return Err("checksum mismatch".to_owned());
    }
    Ok(())
}

fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.').map(|part| part.parse::<u64>().ok());
    let parsed = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(parsed)
}

fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse_version(candidate), parse_version(current)), (Some(a), Some(b)) if a > b)
}

fn battery_low() -> bool {
    let read = |path: PathBuf| fs::read_to_string(path).map(|text| text.trim().to_owned()).unwrap_or_default();
    let Ok(supplies) = fs::read_dir("/sys/class/power_supply") else {
        return false;
    };
    for supply in supplies.flatten().map(|entry| entry.path()) {
        if read(supply.join("type")) == "Battery" {
            let capacity = read(supply.join("capacity")).parse::<u32>().unwrap_or(100);
            return capacity < MIN_BATTERY && read(supply.join("status")) == "Discharging";
        }
    }
    false
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = "RWQPGzLunWlEyJszCbzBEnPHhckebib5svXAXoh1IuhaqUs1Q5Yp2AYJ";
    const TEST_MANIFEST: &str = r#"{"version":"9.9.9","size":5,"sha256":"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824","urls":["https://relio.androne.dev/KoboRoot.tgz"]}"#;
    const TEST_SIGNATURE: &str = "untrusted comment: signature from minisign secret key
RUQPGzLunWlEyGlPIhsFwFRa2xY7R64RzW7N3FaYyYyo7ZHW0okUT4NzrkYB90Io8W/sQ6vEZriK/sXGwk6NsOsBs18y0U1i6Qc=
trusted comment: relio test manifest
ucfPnQVdXD0LGgGatALw9nBCMcUrCMLIGa7RHbY66rt+ceHVrFj39WUX/wkBt8X2yLpx8zSlSDk0tsZct578DA==
";

    fn test_key() -> PublicKey {
        PublicKey::from_base64(TEST_KEY).unwrap()
    }

    fn updater(dir: &Path) -> Updater {
        Updater::with_paths(updater_paths(dir), true)
    }

    fn updater_paths(dir: &Path) -> Paths {
        fs::create_dir_all(dir.join(".kobo")).unwrap();
        Paths {
            package: dir.join(".kobo/KoboRoot.tgz"),
            partial: dir.join(".kobo/relio-update.part"),
            saved: dir.join("update.json"),
            opt_out: dir.join("no-updates"),
            rolled_back: dir.join("rolled-back"),
            ready_flag: dir.join("ready"),
            backup: Some((dir.join("reliod"), dir.join("reliod.prev"))),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("reliod-update-{name}-{}", crate::token::random_token()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn staged(dir: &Path, version: &str) {
        fs::write(dir.join("update.json"), format!(r#"{{"last_check":1,"staged":"{version}"}}"#)).unwrap();
    }

    #[test]
    fn release_key_is_valid() {
        PublicKey::from_base64(RELEASE_KEY).unwrap();
    }

    #[test]
    fn accepts_a_signed_manifest() {
        let manifest = parse_manifest(TEST_MANIFEST.as_bytes(), TEST_SIGNATURE, &test_key()).unwrap();
        assert_eq!(manifest.version, "9.9.9");
        assert_eq!(manifest.size, 5);
    }

    #[test]
    fn rejects_an_altered_manifest() {
        let altered = TEST_MANIFEST.replace("9.9.9", "9.9.8");
        assert!(parse_manifest(altered.as_bytes(), TEST_SIGNATURE, &test_key()).is_err());
    }

    #[test]
    fn rejects_another_key() {
        let other = PublicKey::from_base64("RWQ1xIO2aZN5onQVMiiFOzDPUjLFKVLq5d1XXlT4mQjwieXNzyfysobW").unwrap();
        assert!(parse_manifest(TEST_MANIFEST.as_bytes(), TEST_SIGNATURE, &other).is_err());
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.10.0", "0.9.3"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("0.3", "0.2.0"));
        assert!(!is_newer("0.3.0-beta", "0.2.0"));
    }

    #[test]
    fn checks_size_and_checksum() {
        let dir = scratch("verify");
        let manifest = parse_manifest(TEST_MANIFEST.as_bytes(), TEST_SIGNATURE, &test_key()).unwrap();
        let file = || File::create(dir.join("part")).unwrap();
        assert!(write_verified(&b"hello"[..], file(), &manifest).is_ok());
        assert!(write_verified(&b"hellO"[..], file(), &manifest).is_err());
        assert!(write_verified(&b"hello!"[..], file(), &manifest).is_err());
        assert!(write_verified(&b"hell"[..], file(), &manifest).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    /// Serves fixed bodies by path, one request per connection.
    fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> String {
        use std::io::BufRead;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut request = String::new();
                std::io::BufReader::new(&stream).read_line(&mut request).unwrap();
                let path = request.split_whitespace().nth(1).unwrap_or_default().to_owned();
                let body = routes.iter().find(|(route, _)| *route == path).map(|(_, body)| body.clone());
                let head = match &body {
                    Some(body) => format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()),
                    None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
                };
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body.unwrap_or_default());
            }
        });
        base
    }

    #[test]
    fn fetches_verifies_and_stages_a_release() {
        let base = serve(vec![
            ("/update.json", TEST_MANIFEST.as_bytes().to_vec()),
            ("/update.json.minisig", TEST_SIGNATURE.as_bytes().to_vec()),
            ("/KoboRoot.tgz", b"hello".to_vec()),
        ]);
        let agent = agent(false);
        let missing = format!("{base}/missing.json");
        let listed = format!("{base}/update.json");
        let mut manifest = fetch_manifest(&agent, &[&missing, &listed], &test_key()).unwrap();
        assert_eq!(manifest.version, "9.9.9");

        let dir = scratch("release");
        let updater = updater(&dir);
        fs::write(dir.join("reliod"), "current").unwrap();
        manifest.urls = vec![format!("{base}/gone.tgz"), format!("{base}/KoboRoot.tgz")];
        updater.download(&agent, &manifest).unwrap();
        updater.stage().unwrap();
        assert_eq!(fs::read_to_string(dir.join(".kobo/KoboRoot.tgz")).unwrap(), "hello");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn manual_checks_wake_the_loop_and_show_progress() {
        let dir = scratch("manual");
        let updater = std::sync::Arc::new(updater(&dir));
        assert!(updater.request_check());
        assert_eq!(updater.status(), Status::Checking);
        let started = std::time::Instant::now();
        updater.wait(Duration::from_secs(30));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!Updater::with_paths(updater_paths(&dir), false).request_check());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn confirms_an_installed_version() {
        let dir = scratch("installed");
        let updater = updater(&dir);
        staged(&dir, VERSION);
        fs::write(dir.join("reliod.prev"), "old").unwrap();
        let saved = updater.reconcile();
        assert_eq!(saved.staged, None);
        assert!(!dir.join("reliod.prev").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn skips_a_version_that_was_rolled_back() {
        let dir = scratch("rollback");
        let updater = updater(&dir);
        staged(&dir, "99.0.0");
        fs::write(dir.join("rolled-back"), "").unwrap();
        let saved = updater.reconcile();
        assert_eq!(saved.staged, None);
        assert_eq!(saved.skipped.as_deref(), Some("99.0.0"));
        assert!(!dir.join("rolled-back").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_a_waiting_package_after_an_unrelated_restart() {
        let dir = scratch("unrelated");
        let updater = updater(&dir);
        staged(&dir, "99.0.0");
        fs::write(dir.join("rolled-back"), "").unwrap();
        fs::write(dir.join(".kobo/KoboRoot.tgz"), "package").unwrap();
        let saved = updater.reconcile();
        assert_eq!(saved.skipped, None);
        assert_eq!(updater.status(), Status::Ready { version: "99.0.0".to_owned() });
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_a_package_still_waiting() {
        let dir = scratch("waiting");
        let updater = updater(&dir);
        staged(&dir, "99.0.0");
        fs::write(dir.join(".kobo/KoboRoot.tgz"), "package").unwrap();
        fs::write(dir.join("reliod.prev"), "old").unwrap();
        updater.reconcile();
        assert_eq!(updater.status(), Status::Ready { version: "99.0.0".to_owned() });
        assert!(dir.join("reliod.prev").exists());
        assert_eq!(fs::read_to_string(dir.join("ready")).unwrap(), "99.0.0");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stages_next_to_the_kobo_files() {
        let dir = scratch("stage");
        let updater = updater(&dir);
        fs::write(dir.join("reliod"), "current").unwrap();
        fs::write(dir.join(".kobo/relio-update.part"), "package").unwrap();
        updater.stage().unwrap();
        assert_eq!(fs::read_to_string(dir.join(".kobo/KoboRoot.tgz")).unwrap(), "package");
        assert_eq!(fs::read_to_string(dir.join("reliod.prev")).unwrap(), "current");
        assert!(!dir.join(".kobo/relio-update.part").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
