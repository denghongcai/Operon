use clap::Parser;

mod cli_args;
mod cli_dispatch;
mod commands;
mod graph;
mod grpc;
mod grpc_audit;
mod grpc_exec;
mod grpc_exec_api;
mod grpc_fs;
mod grpc_service;
mod grpc_service_api;
mod onboard;
mod output;
mod private_files;
mod target;

pub(crate) use output::{print_json, OutputMode};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli_dispatch::dispatch(cli_args::Args::parse()).await
}
