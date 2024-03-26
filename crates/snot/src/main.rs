use std::io;

use clap::Parser;
use cli::Cli;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;

pub mod cannon;
pub mod cli;
pub mod env;
pub mod schema;
pub mod server;
pub mod state;

#[tokio::main]
async fn main() {
    let env_filter = if cfg!(debug_assertions) {
        tracing_subscriber::EnvFilter::builder().with_default_directive(LevelFilter::TRACE.into())
    } else {
        tracing_subscriber::EnvFilter::builder().with_default_directive(LevelFilter::INFO.into())
    };

    let env_filter = env_filter
        .parse_lossy("")
        .add_directive("surrealdb_core=off".parse().unwrap())
        .add_directive("surrealdb=off".parse().unwrap())
        .add_directive("tungstenite=off".parse().unwrap())
        .add_directive("tokio_tungstenite=off".parse().unwrap())
        .add_directive("tarpc::client=ERROR".parse().unwrap())
        .add_directive("tarpc::server=ERROR".parse().unwrap());

    let (stdout, _guard) = tracing_appender::non_blocking(io::stdout());

    let output = tracing_subscriber::fmt::layer().with_writer(stdout);

    let output = if cfg!(debug_assertions) {
        output.with_file(true).with_line_number(true)
    } else {
        output
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(output)
        .try_init()
        .unwrap();

    let cli = Cli::parse();

    server::start(cli).await.expect("start server");
}
