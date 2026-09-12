//! `bridge-provider` binary entrypoint.
//!
//! Loads config from `BRIDGE_CONFIG` (default `bridge.toml`), initializes tracing
//! with an [`EnvFilter`] from `RUST_LOG` (falls back to the `info` level when the
//! variable is unset), and serves the webhook router on `config.listen` via axum.
//!
//! # Graceful shutdown
//!
//! On SIGINT (ctrl_c) or — on Unix — SIGTERM, axum stops accepting new
//! connections, then [`RunRegistry::abort_all`] cancels every in-flight run
//! (`aborted` terminal state); each run loop finalizes, emits the terminal SSE
//! frame, reaps its engine subprocess, and the in-flight HTTP connections drain.
//! The process then exits 0. No `unwrap`/`expect`/`panic` lives in this file.
//!
//! # HTTP/2 (h2c) — manual verification, NOT in CI
//!
//! Production BCN Provider 2.0 speaks HTTP/2 cleartext (h2c). `axum::serve`
//! drives hyper-util's auto connection builder, which detects the h2 connection
//! preface and upgrades to h2c — so a `--http2-prior-knowledge` client is served
//! as HTTP/2 without TLS. Verify locally against a running binary:
//!
//! ```text
//! # write a throwaway config bound to 127.0.0.1:21999
//! cat > /tmp/bridge-h2c.toml <<'EOF'
//! provider_id        = "bridge-1"
//! listen             = "127.0.0.1:21999"
//! bcs_to_provider_token = "tok-b2p"
//! [[bot]]
//! provider_bot_ref = "worker-1"
//! engine = "cfuse-cc"
//! cwd = "/tmp"
//! EOF
//! BRIDGE_CONFIG=/tmp/bridge-h2c.toml cargo run -p bridge-provider &
//! # send a bot.ping over h2c prior-knowledge (no TLS):
//! curl --http2-prior-knowledge -sS -v \
//!   -H 'Authorization: Bearer tok-b2p' \
//!   -H 'Content-Type: application/json' \
//!   --data '{"type":"req","id":"p1","method":"bot.ping","to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"}}' \
//!   http://127.0.0.1:21999/webhook
//! # expect:  * Using HTTP2 prior knowledge
//! #          < HTTP/2 200
//! #          {"ok":true}
//! # SIGTERM the binary; it should exit 0.
//! kill -TERM %1
//! ```

use std::{path::PathBuf, sync::Arc};

use bridge_provider::{config::ProviderConfig, webhook, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config_path: PathBuf = std::env::var("BRIDGE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("bridge.toml"));
    let config = ProviderConfig::load(&config_path)?;
    let listen = config.listen;
    let state = Arc::new(AppState::new(config));
    let app = webhook::router(state.clone());

    let listener = tokio::net::TcpListener::bind(listen).await?;
    tracing::info!(%listen, "bridge-provider listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            // axum stops taking new connections; cancel every in-flight run so
            // the run loops finalize (aborted terminal) and their engine
            // subprocesses are reaped before the process exits.
            state.runs.abort_all("shutdown").await;
        })
        .await?;
    Ok(())
}

/// Wait for SIGINT (ctrl_c) or — on Unix — SIGTERM, whichever arrives first.
///
/// Split by `#[cfg(unix)]` so non-Unix builds still compile against `ctrl_c`.
/// Installing the SIGTERM handler never panics; on the (effectively unreachable
/// for Linux) failure path it falls back to ctrl_c-only before returning.
#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "failed to install SIGTERM handler; falling back to ctrl_c");
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!(signal = "SIGINT", "graceful shutdown initiated");
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!(signal = "SIGINT", "graceful shutdown initiated"),
        _ = sigterm.recv() => tracing::info!(signal = "SIGTERM", "graceful shutdown initiated"),
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!(signal = "SIGINT", "graceful shutdown initiated");
}
