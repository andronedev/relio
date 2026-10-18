//! `kobo-touch info [device]` prints the touchscreen axes;
//! `kobo-touch tap X Y [device]` taps at raw touchscreen coordinates;
//! `kobo-touch record SECONDS [device]` prints incoming events.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

const DEFAULT_DEVICE: &str = "/dev/input/event1";

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0;
const SYN_MT_REPORT: u16 = 2;
const BTN_TOUCH: u16 = 0x14a;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_PRESSURE: u16 = 0x18;
const ABS_MT_TOUCH_MAJOR: u16 = 0x30;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const ABS_MT_PRESSURE: u16 = 0x3a;

const AXES: &[(u16, &str)] = &[
    (ABS_X, "ABS_X"),
    (ABS_Y, "ABS_Y"),
    (ABS_PRESSURE, "ABS_PRESSURE"),
    (ABS_MT_TOUCH_MAJOR, "ABS_MT_TOUCH_MAJOR"),
    (ABS_MT_POSITION_X, "ABS_MT_POSITION_X"),
    (ABS_MT_POSITION_Y, "ABS_MT_POSITION_Y"),
    (ABS_MT_TRACKING_ID, "ABS_MT_TRACKING_ID"),
    (ABS_MT_PRESSURE, "ABS_MT_PRESSURE"),
];

#[repr(C)]
#[derive(Default)]
struct AbsInfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

fn eviocgabs(axis: u16) -> libc::c_ulong {
    let size = std::mem::size_of::<AbsInfo>() as libc::c_ulong;
    (2 << 30) | (size << 16) | ((b'E' as libc::c_ulong) << 8) | (0x40 + axis as libc::c_ulong)
}

fn info(device: &str) -> std::io::Result<()> {
    let file = File::open(device)?;
    for &(axis, name) in AXES {
        let mut abs = AbsInfo::default();
        let result = unsafe { libc::ioctl(file.as_raw_fd(), eviocgabs(axis) as _, &mut abs) };
        if result == 0 {
            println!("{name}: min {} max {} res {}", abs.minimum, abs.maximum, abs.resolution);
        }
    }
    Ok(())
}

/// 32-bit kernel ABI: two 32-bit time fields, type, code, value.
fn event(kind: u16, code: u16, value: i32) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[8..10].copy_from_slice(&kind.to_le_bytes());
    bytes[10..12].copy_from_slice(&code.to_le_bytes());
    bytes[12..16].copy_from_slice(&value.to_le_bytes());
    bytes
}

fn tap(device: &str, x: i32, y: i32) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).open(device)?;
    let press = [
        event(EV_ABS, ABS_MT_TRACKING_ID, 0),
        event(EV_ABS, ABS_MT_TOUCH_MAJOR, 10),
        event(EV_ABS, ABS_MT_PRESSURE, 60),
        event(EV_ABS, ABS_MT_POSITION_X, x),
        event(EV_ABS, ABS_MT_POSITION_Y, y),
        event(EV_SYN, SYN_MT_REPORT, 0),
        event(EV_KEY, BTN_TOUCH, 1),
        event(EV_SYN, SYN_REPORT, 0),
    ];
    let release = [
        event(EV_SYN, SYN_MT_REPORT, 0),
        event(EV_KEY, BTN_TOUCH, 0),
        event(EV_SYN, SYN_REPORT, 0),
    ];
    file.write_all(&press.concat())?;
    std::thread::sleep(Duration::from_millis(80));
    file.write_all(&release.concat())
}

fn record(device: &str, seconds: u64) -> std::io::Result<()> {
    let mut file = File::open(device)?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut buffer = [0u8; 16];
    unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) };
    while Instant::now() < deadline {
        match file.read_exact(&mut buffer) {
            Ok(()) => {
                let kind = u16::from_le_bytes([buffer[8], buffer[9]]);
                let code = u16::from_le_bytes([buffer[10], buffer[11]]);
                let value = i32::from_le_bytes([buffer[12], buffer[13], buffer[14], buffer[15]]);
                println!("type {kind:#04x} code {code:#05x} value {value}");
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    let number = |i: usize| arg(i).and_then(|v| v.parse::<i32>().ok());
    let result = match arg(0) {
        Some("info") => info(arg(1).unwrap_or(DEFAULT_DEVICE)),
        Some("tap") => match (number(1), number(2)) {
            (Some(x), Some(y)) => tap(arg(3).unwrap_or(DEFAULT_DEVICE), x, y),
            _ => Err(std::io::Error::other("usage: kobo-touch tap X Y [device]")),
        },
        Some("record") => record(arg(2).unwrap_or(DEFAULT_DEVICE), number(1).unwrap_or(5) as u64),
        _ => Err(std::io::Error::other("usage: kobo-touch info|tap X Y|record SECONDS [device]")),
    };
    if let Err(err) = result {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
