//! Entry: load config, ensure ~/.cursor-brain/workspace exists, write PID, start HTTP server.

mod config;
mod cursor;
mod metrics;
mod openai;
mod server;
mod service;
mod session;

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn ensure_minimal_workspace_dir(config: &config::Config) {
    let dir = config
        .workspace_dir_for_spawn()
        .unwrap_or_else(config::default_minimal_workspace_dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("could not create minimal workspace dir {}: {}", dir, e);
    }
}

/// Writes PID file: create if not exists, or truncate then write (never delete-then-create).
fn write_pid_file() {
    let path = config::pid_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let pid = std::process::id();
    if let Err(e) = std::fs::write(&path, pid.to_string()) {
        tracing::warn!("could not write PID file {}: {}", path.display(), e);
    }
}

fn remove_pid_file() {
    let path = config::pid_file_path();
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
}

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|e| {
        eprintln!("cursor-brain: invalid RUST_LOG ({}), using 'info'", e);
        EnvFilter::new("info")
    });
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    std::panic::set_hook(Box::new(|_| {
        remove_pid_file();
    }));

    let config = Arc::new(config::load_config());
    ensure_minimal_workspace_dir(&config);

    let port = config.port;
    let bind_addr = std::net::IpAddr::from_str(config.bind_address.as_str())
        .unwrap_or_else(|_| std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)));
    let addr = SocketAddr::new(bind_addr, port);

    let app = server::app(config.clone());
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    write_pid_file();

    tracing::info!(
        "cursor-brain {} listening on http://{}",
        server::CURSOR_BRAIN_VERSION,
        listener.local_addr().unwrap()
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("serve");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.expect("ctrl_c");
    tracing::info!("shutting down");
    remove_pid_file();
}
