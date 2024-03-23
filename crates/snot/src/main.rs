use clap::Parser;
use cli::Cli;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;

pub mod cli;
pub mod schema;
pub mod server;
pub mod state;
pub mod testing;

#[tokio::main]
async fn main() {
    let env_filter = if cfg!(debug_assertions) {
        tracing_subscriber::EnvFilter::builder().with_default_directive(LevelFilter::TRACE.into())
    } else {
        tracing_subscriber::EnvFilter::builder().with_default_directive(LevelFilter::INFO.into())
    };

    let env_filter = env_filter
        .parse_lossy("")
        .add_directive("tarpc::client=ERROR".parse().unwrap())
        .add_directive("tarpc::server=ERROR".parse().unwrap());

    let output = tracing_subscriber::fmt::layer();

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
