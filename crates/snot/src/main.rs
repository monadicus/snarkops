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
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .parse_lossy("")
                .add_directive("tarpc::client=ERROR".parse().unwrap())
                .add_directive("tarpc::server=ERROR".parse().unwrap()),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .unwrap();

    let cli = Cli::parse();

    server::start(cli).await.expect("start server");
}
