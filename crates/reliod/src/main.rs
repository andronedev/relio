mod api;
mod config;
mod covers;
mod files;
mod pairing;
mod screen;
mod token;
mod update;
mod web;

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;

use api::AppState;
use config::Config;
use pairing::Pairing;
use relio_core::Library;
use update::Updater;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let pair_only = args.first().is_some_and(|a| a == "pair");
    if pair_only {
        args.remove(0);
    }
    let config = Config::from_args(args).unwrap_or_else(|err| exit(2, &err));

    if pair_only {
        open_pairing(&config.control_socket);
        return;
    }

    let pairing = Pairing::load(config.state_dir.clone())
        .unwrap_or_else(|err| exit(1, &format!("cannot load {}: {err}", config.state_dir.display())));
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("cannot start runtime")
        .block_on(serve(config, pairing));
}

async fn serve(config: Config, pairing: Pairing) {
    let public = tokio::net::TcpListener::bind(config.listen)
        .await
        .unwrap_or_else(|err| exit(1, &format!("cannot listen on {}: {err}", config.listen)));
    let _ = std::fs::remove_file(&config.control_socket);
    let control = tokio::net::UnixListener::bind(&config.control_socket)
        .unwrap_or_else(|err| exit(1, &format!("cannot bind {}: {err}", config.control_socket.display())));
    println!(
        "reliod {} serving {} on http://{} (control socket {})",
        update::VERSION,
        config.db.display(),
        config.listen,
        config.control_socket.display()
    );

    let updater = Arc::new(Updater::new(&config.onboard, &config.state_dir, config.update_flag()));
    updater.clone().spawn();

    let state = Arc::new(AppState {
        library: Library::new(&config.db),
        version_file: config.version_file(),
        files: files::BookFiles::new(config.books_dir(), config.rescan_trigger()),
        covers: covers::Covers::new(config.onboard.join(".kobo-images")),
        reading_flag: config.reading_flag(),
        port: config.listen.port(),
        onboard: config.onboard,
        pairing,
        updater,
    });
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    tokio::select! {
        result = axum::serve(public, api::public_router(state.clone())) => result.expect("public server error"),
        result = axum::serve(control, api::control_router(state)) => result.expect("control server error"),
        () = shutdown => {}
    }
}

/// Stand-in for the status-bar menu entry: asks the running daemon for a pairing code.
fn open_pairing(control_socket: &Path) {
    let mut stream = UnixStream::connect(control_socket)
        .unwrap_or_else(|err| exit(1, &format!("reliod is not running on {}: {err}", control_socket.display())));
    let mut response = String::new();
    stream
        .write_all(b"POST /pairing HTTP/1.1\r\nHost: reliod\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .and_then(|()| stream.read_to_string(&mut response))
        .unwrap_or_else(|err| exit(1, &format!("pairing request failed: {err}")));
    match response.split_once("\r\n\r\n") {
        Some((head, body)) if head.starts_with("HTTP/1.1 200") => println!("{body}"),
        _ => exit(1, &format!("unexpected response: {response}")),
    }
}

fn exit(code: i32, message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(code)
}
