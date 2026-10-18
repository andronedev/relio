use std::net::SocketAddr;
use std::path::PathBuf;

const DEVICE_ONBOARD: &str = "/mnt/onboard";
const DEFAULT_CONTROL_SOCKET: &str = "/tmp/reliod.sock";
const RESCAN_TRIGGER: &str = "/tmp/relio-rescan";
const READING_FLAG: &str = "/tmp/relio-reading";
const UPDATE_FLAG: &str = "/tmp/relio-update-ready";
const USAGE: &str = "usage: reliod [pair] [--onboard dir] [--db file] [--listen addr] [--state dir] [--control socket]";

#[derive(Debug, Clone)]
pub struct Config {
    pub onboard: PathBuf,
    pub db: PathBuf,
    pub listen: SocketAddr,
    pub state_dir: PathBuf,
    pub control_socket: PathBuf,
}

impl Config {
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut onboard = PathBuf::from(DEVICE_ONBOARD);
        let mut db = None;
        let mut state_dir = None;
        let mut control_socket = PathBuf::from(DEFAULT_CONTROL_SOCKET);
        let mut listen: SocketAddr = ([0, 0, 0, 0], 8080).into();

        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            let value = args.next().ok_or_else(|| format!("missing value for {flag}\n{USAGE}"))?;
            match flag.as_str() {
                "--onboard" => onboard = value.into(),
                "--db" => db = Some(value.into()),
                "--state" => state_dir = Some(value.into()),
                "--listen" => listen = value.parse().map_err(|e| format!("--listen: {e}"))?,
                "--control" => control_socket = value.into(),
                _ => return Err(format!("unknown flag {flag}\n{USAGE}")),
            }
        }

        Ok(Self {
            db: db.unwrap_or_else(|| onboard.join(".kobo/KoboReader.sqlite")),
            state_dir: state_dir.unwrap_or_else(|| onboard.join(".adds/relio")),
            onboard,
            listen,
            control_socket,
        })
    }

    pub fn version_file(&self) -> PathBuf {
        self.onboard.join(".kobo/version")
    }

    /// Added books go to the root of the user partition, like books copied over USB.
    pub fn books_dir(&self) -> PathBuf {
        self.onboard.clone()
    }

    /// Watched by the Nickel plugin, which then asks Nickel to import new or removed books.
    pub fn rescan_trigger(&self) -> PathBuf {
        PathBuf::from(RESCAN_TRIGGER)
    }

    /// Present while a book is open in Nickel, maintained by the plugin.
    pub fn reading_flag(&self) -> PathBuf {
        PathBuf::from(READING_FLAG)
    }

    /// Present while an update waits for a restart; the plugin then marks its icon.
    pub fn update_flag(&self) -> PathBuf {
        PathBuf::from(UPDATE_FLAG)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn defaults_to_device_layout() {
        let config = Config::from_args(args(&[])).unwrap();
        assert_eq!(config.db, Path::new("/mnt/onboard/.kobo/KoboReader.sqlite"));
        assert_eq!(config.state_dir, Path::new("/mnt/onboard/.adds/relio"));
    }

    #[test]
    fn rejects_unknown_flags() {
        assert!(Config::from_args(args(&["--nope", "x"])).is_err());
    }
}
