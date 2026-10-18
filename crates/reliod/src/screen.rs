use std::net::{IpAddr, UdpSocket};

use qrcode::{Color, QrCode};
use serde::Serialize;

use crate::pairing::{PairingState, PendingCode};

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PairingView {
    Idle,
    Waiting(PairingScreen),
    Paired { name: String },
}

impl PairingView {
    pub fn new(state: PairingState, port: u16) -> Self {
        match state {
            PairingState::Idle => Self::Idle,
            PairingState::Waiting(pending) => Self::Waiting(PairingScreen::new(pending, port)),
            PairingState::Paired { name } => Self::Paired { name },
        }
    }
}

/// Everything the e-reader UI needs to draw the pairing screen.
#[derive(Debug, Serialize)]
pub struct PairingScreen {
    pub code: String,
    pub expires_in: u64,
    pub url: Option<String>,
    pub qr: Option<Qr>,
}

#[derive(Debug, Serialize)]
pub struct Qr {
    pub size: usize,
    /// Row-major modules, `1` for dark.
    pub modules: String,
}

impl PairingScreen {
    pub fn new(pending: PendingCode, port: u16) -> Self {
        let url = lan_ip().map(|ip| dashboard_url(ip, port, &pending.code));
        let qr = url.as_deref().and_then(qr);
        Self {
            code: pending.code,
            expires_in: pending.expires_in,
            url,
            qr,
        }
    }
}

fn dashboard_url(ip: IpAddr, port: u16, code: &str) -> String {
    format!("http://{ip}:{port}/#code={code}")
}

/// Connecting a UDP socket only selects a route; no packet is sent.
fn lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("192.0.2.1", 9)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

fn qr(text: &str) -> Option<Qr> {
    let code = QrCode::new(text.as_bytes()).ok()?;
    let modules = code
        .to_colors()
        .into_iter()
        .map(|c| if c == Color::Dark { '1' } else { '0' })
        .collect();
    Some(Qr { size: code.width(), modules })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_scannable_dashboard_link() {
        let url = dashboard_url([192, 168, 0, 42].into(), 8080, "123456");
        assert_eq!(url, "http://192.168.0.42:8080/#code=123456");
        let qr = qr(&url).unwrap();
        assert_eq!(qr.modules.len(), qr.size * qr.size);
    }
}
