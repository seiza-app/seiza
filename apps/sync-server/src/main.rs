// Seiza's sync server (PLAN §8, Sync). It keeps each sync group's devices and their encrypted
// batches, in order, and relays pairing messages between two devices. Everything it stores for a
// group is ciphertext: it can see the group, the device, sizes and times, and nothing else.

mod api;
mod config;
mod db;
mod error;
mod limits;
mod pairing;
mod tokens;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitCode;
use std::time::Duration;

use config::Config;

const HELP: &str = "\
Seiza's sync server.

Usage: seiza-sync [command]

Commands:
  serve    Run the server (the default)
  stats    Count groups, devices and active groups
  invite   Make a one-time invite code for a new group (for SEIZA_SYNC_NEW_GROUPS=invites)
  backup   Copy the database to a new file, safely while the server runs: seiza-sync backup <file>
  health   Check that the server here answers (for Docker's health check)

Settings come from the environment; see README.md.";

fn main() -> ExitCode {
    let command = std::env::args().nth(1);
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("seiza-sync: {error}");
            return ExitCode::FAILURE;
        }
    };
    let result = match command.as_deref() {
        None | Some("serve") => serve(config),
        Some("stats") => stats(&config),
        Some("invite") => invite(&config),
        Some("backup") => backup(&config, std::env::args().nth(2)),
        Some("health") => health(&config),
        Some("help" | "--help" | "-h") => {
            println!("{HELP}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command \"{other}\"\n\n{HELP}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("seiza-sync: {error}");
            ExitCode::FAILURE
        }
    }
}

fn serve(config: Config) -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    runtime.block_on(async move {
        let db = db::open(&config.db_path).map_err(|error| format!("couldn't open the database: {error}"))?;
        let addr = config.addr;
        let app = api::App::new(db, config);
        tokio::spawn(api::sweep_pairings(app.clone()));
        let app = api::router(app);
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|error| format!("couldn't listen on {addr}: {error}"))?;
        println!("seiza-sync: listening on {addr}");
        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(shutdown())
            .await
            .map_err(|error| error.to_string())
    })
}

/// Waits for Ctrl-C or, in Docker, SIGTERM, so requests in flight finish and the database closes
/// cleanly.
async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

fn stats(config: &Config) -> Result<(), String> {
    let db = db::open(&config.db_path).map_err(|error| error.to_string())?;
    let stats = db::stats(&db, tokens::now_ms()).map_err(|error| error.to_string())?;
    println!("Groups:                  {}", stats.groups);
    println!("Devices:                 {}", stats.devices);
    println!("Groups active, 7 days:   {}", stats.active_7_days);
    println!("Groups active, 30 days:  {}", stats.active_30_days);
    println!("Stored:                  {:.1} MB", stats.bytes as f64 / 1_000_000.0);
    Ok(())
}

fn invite(config: &Config) -> Result<(), String> {
    let db = db::open(&config.db_path).map_err(|error| error.to_string())?;
    let code = db::make_invite(&db, tokens::now_ms()).map_err(|error| error.to_string())?;
    println!("Invite code for one new sync group, valid for 7 days:\n\n  {code}\n");
    if config.new_groups != config::NewGroups::Invites {
        println!(
            "Note: this server takes new groups by \"{}\", so the code isn't needed.",
            config.new_groups
        );
        println!("Set SEIZA_SYNC_NEW_GROUPS=invites for invite codes to be asked for.");
    }
    Ok(())
}

fn backup(config: &Config, to: Option<String>) -> Result<(), String> {
    let to = to.ok_or("say where the copy goes, e.g. seiza-sync backup /data/backup.db")?;
    let db = db::open(&config.db_path).map_err(|error| error.to_string())?;
    db::backup(&db, std::path::Path::new(&to)).map_err(|error| format!("couldn't back up to {to}: {error}"))?;
    println!("Backed up to {to}");
    Ok(())
}

/// Asks this machine's server for /health, with nothing beyond the standard library, so the
/// Docker image needs no curl.
fn health(config: &Config) -> Result<(), String> {
    let port = config.addr.port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(b"GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .map_err(|error| error.to_string())?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).map_err(|error| error.to_string())?;
    if reply.starts_with("HTTP/1.0 200") || reply.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        Err(format!("unexpected reply: {}", reply.lines().next().unwrap_or("")))
    }
}
