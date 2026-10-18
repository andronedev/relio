use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::token::{hash_token, random_code, random_token};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

const CODE_DIGITS: u32 = 6;
const CODE_LIFETIME: Duration = Duration::from_secs(180);
const MAX_ATTEMPTS: u8 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub paired_at: u64,
    token_hash: String,
}

#[derive(Debug, Serialize)]
pub struct PendingCode {
    pub code: String,
    pub expires_in: u64,
}

#[derive(Debug)]
pub enum PairingState {
    Idle,
    Waiting(PendingCode),
    Paired { name: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum PairError {
    NoPairingInProgress,
    WrongCode,
}

struct Pending {
    code: String,
    expires_at: Instant,
    attempts_left: u8,
}

impl Pending {
    fn is_live(&self) -> bool {
        Instant::now() < self.expires_at && self.attempts_left > 0
    }
}

/// Pairing is opened from the e-reader, which shows a short-lived code; a browser
/// that types it receives its own revocable token.
pub struct Pairing {
    clients_file: PathBuf,
    pending: Mutex<Option<Pending>>,
    last_paired: Mutex<Option<String>>,
    clients: Mutex<Vec<Client>>,
}

impl Pairing {
    pub fn load(state_dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&state_dir)?;
        let clients_file = state_dir.join("clients.json");
        let clients = match std::fs::read(&clients_file) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(std::io::Error::other)?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(err) => return Err(err),
        };
        Ok(Self {
            clients_file,
            pending: Mutex::new(None),
            last_paired: Mutex::new(None),
            clients: Mutex::new(clients),
        })
    }

    pub fn start(&self) -> PendingCode {
        *self.last_paired.lock().unwrap() = None;
        let code = random_code(CODE_DIGITS);
        *self.pending.lock().unwrap() = Some(Pending {
            code: code.clone(),
            expires_at: Instant::now() + CODE_LIFETIME,
            attempts_left: MAX_ATTEMPTS,
        });
        PendingCode {
            code,
            expires_in: CODE_LIFETIME.as_secs(),
        }
    }

    pub fn current(&self) -> Option<PendingCode> {
        let pending = self.pending.lock().unwrap();
        pending.as_ref().filter(|p| p.is_live()).map(|p| PendingCode {
            code: p.code.clone(),
            expires_in: p.expires_at.saturating_duration_since(Instant::now()).as_secs(),
        })
    }

    pub fn state(&self) -> PairingState {
        if let Some(pending) = self.current() {
            return PairingState::Waiting(pending);
        }
        match self.last_paired.lock().unwrap().clone() {
            Some(name) => PairingState::Paired { name },
            None => PairingState::Idle,
        }
    }

    pub fn cancel(&self) {
        *self.pending.lock().unwrap() = None;
        *self.last_paired.lock().unwrap() = None;
    }

    pub fn confirm(&self, code: &str, name: &str) -> Result<String, PairError> {
        {
            let mut pending = self.pending.lock().unwrap();
            let current = pending
                .as_mut()
                .filter(|p| p.is_live())
                .ok_or(PairError::NoPairingInProgress)?;
            if !bool::from(current.code.as_bytes().ct_eq(code.trim().as_bytes())) {
                current.attempts_left -= 1;
                return Err(PairError::WrongCode);
            }
            *pending = None;
        }

        let token = random_token();
        let client = Client {
            id: random_token()[..12].to_owned(),
            name: clean_name(name),
            paired_at: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            token_hash: hash_token(&token),
        };
        *self.last_paired.lock().unwrap() = Some(client.name.clone());
        let mut clients = self.clients.lock().unwrap();
        clients.push(client);
        self.save(&clients);
        Ok(token)
    }

    pub fn authenticate(&self, token: &str) -> bool {
        let presented = hash_token(token);
        self.clients
            .lock()
            .unwrap()
            .iter()
            .any(|c| bool::from(c.token_hash.as_bytes().ct_eq(presented.as_bytes())))
    }

    pub fn clients(&self) -> Vec<Client> {
        self.clients.lock().unwrap().clone()
    }

    pub fn revoke(&self, id: &str) -> bool {
        let mut clients = self.clients.lock().unwrap();
        let before = clients.len();
        clients.retain(|c| c.id != id);
        let removed = clients.len() != before;
        if removed {
            self.save(&clients);
        }
        removed
    }

    fn save(&self, clients: &[Client]) {
        let result = serde_json::to_vec_pretty(clients)
            .map_err(std::io::Error::other)
            .and_then(|json| {
                let tmp = self.clients_file.with_extension("tmp");
                std::fs::write(&tmp, json)?;
                std::fs::rename(&tmp, &self.clients_file)
            });
        if let Err(err) = result {
            eprintln!("cannot save paired clients: {err}");
        }
    }
}

fn clean_name(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).take(60).collect();
    let name = name.trim();
    if name.is_empty() { "Device".to_owned() } else { name.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairing() -> Pairing {
        let dir = std::env::temp_dir().join(format!("reliod-pairing-{}-{}", std::process::id(), random_token()));
        Pairing::load(dir).unwrap()
    }

    #[test]
    fn pairs_with_the_displayed_code() {
        let pairing = pairing();
        let code = pairing.start().code;
        let token = pairing.confirm(&code, "Chrome on macOS").unwrap();
        assert!(pairing.authenticate(&token));
        assert!(matches!(pairing.state(), PairingState::Paired { name } if name == "Chrome on macOS"));
        assert!(!pairing.authenticate("forged"));
        assert_eq!(pairing.confirm(&code, "again"), Err(PairError::NoPairingInProgress));
    }

    #[test]
    fn locks_after_too_many_wrong_codes() {
        let pairing = pairing();
        let code = pairing.start().code;
        let wrong = if code == "000000" { "111111" } else { "000000" };
        for _ in 0..MAX_ATTEMPTS {
            assert_eq!(pairing.confirm(wrong, "x"), Err(PairError::WrongCode));
        }
        assert_eq!(pairing.confirm(&code, "x"), Err(PairError::NoPairingInProgress));
        assert!(matches!(pairing.state(), PairingState::Idle));
    }

    #[test]
    fn revoked_clients_lose_access_and_survive_restart() {
        let pairing = pairing();
        let code = pairing.start().code;
        let token = pairing.confirm(&code, "Phone").unwrap();
        let reloaded = Pairing::load(pairing.clients_file.parent().unwrap().to_owned()).unwrap();
        assert!(reloaded.authenticate(&token));
        let id = reloaded.clients()[0].id.clone();
        assert!(reloaded.revoke(&id));
        assert!(!reloaded.authenticate(&token));
    }
}
