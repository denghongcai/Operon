use std::{env, net::SocketAddr};

use axum::{extract::State, routing::get, Json, Router};
use clap::{Parser, Subcommand};
use operon_core::{HealthStatus, NodeInfo};

#[derive(Debug, Parser)]
#[command(name = "operond", about = "Operon capability daemon")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Start(StartArgs),
}

#[derive(Debug, Parser)]
struct StartArgs {
    #[arg(long, default_value = "127.0.0.1:7788")]
    listen: SocketAddr,

    #[arg(long, default_value = "local")]
    node_id: String,
}

#[derive(Debug, Clone)]
struct AppState {
    node: NodeInfo,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    match Args::parse().command {
        Command::Start(args) => start(args).await,
    }
}

async fn start(args: StartArgs) -> anyhow::Result<()> {
    let state = AppState {
        node: NodeInfo {
            id: args.node_id,
            hostname: hostname(),
            os: env::consts::OS.to_string(),
            arch: env::consts::ARCH.to_string(),
        },
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/node", get(node_info))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!("operond listening on {}", args.listen);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<HealthStatus> {
    Json(HealthStatus {
        ok: true,
        node_id: state.node.id,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn node_info(State(state): State<AppState>) -> Json<NodeInfo> {
    Json(state.node)
}

fn hostname() -> String {
    env::var("HOSTNAME")
        .or_else(|_| env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
