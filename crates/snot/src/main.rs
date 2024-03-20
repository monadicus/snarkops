use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;

pub mod schema;
pub mod server;
pub mod state;
pub mod storage;

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

    // TODO: REST api for interacting with CLI/web

    // TODO: possibly need authorization for the REST server for MVP?

    // TODO ws server for talking to runners, runners get data from control
    // plane thru HTTP, not ws

    server::start().await.expect("start server");
}
