use tracing_subscriber::prelude::*;

pub mod schema;
pub mod storage;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .unwrap();

    // TODO: REST api for interacting with CLI/web

    // TODO: possibly need authorization for the REST server for MVP?

    // TODO ws server for talking to runners, runners get data from control
    // plane thru HTTP, not ws
}
