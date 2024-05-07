use std::io;

use clap::Parser;
use cli::Cli;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;

pub mod cannon;
pub mod cli;
pub mod db;
pub mod env;
pub mod error;
pub mod logging;
pub mod schema;
pub mod server;
pub mod state;
pub mod util;

#[tokio::main]
async fn main() {
    let env_filter = if cfg!(debug_assertions) {
        tracing_subscriber::EnvFilter::builder().with_default_directive(LevelFilter::TRACE.into())
    } else {
        tracing_subscriber::EnvFilter::builder().with_default_directive(LevelFilter::INFO.into())
    };

    let env_filter = env_filter
        .with_env_var("SNOPS_LOG")
        .from_env_lossy()
        .add_directive("hyper_util=off".parse().unwrap())
        .add_directive("hyper=off".parse().unwrap())
        .add_directive("reqwest=off".parse().unwrap())
        .add_directive("tungstenite=off".parse().unwrap())
        .add_directive("tokio_tungstenite=off".parse().unwrap())
        .add_directive("tarpc::client=ERROR".parse().unwrap())
        .add_directive("tarpc::server=ERROR".parse().unwrap())
        .add_directive("tower_http::trace::on_request=off".parse().unwrap())
        .add_directive("tower_http::trace::on_response=off".parse().unwrap());

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

    // For documentation purposes will exit after running the command.
    #[cfg(any(feature = "clipages", feature = "mangen"))]
    Cli::parse().run();
    let cli = Cli::parse();

    server::start(cli).await.expect("start server");
}
