use std::{io, sync::Arc};

use clap::Parser;
use cli::Cli;
use surrealdb::Surreal;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;

use crate::state::GlobalState;

pub mod cannon;
pub mod cli;
pub mod env;
pub mod logging;
pub mod prometheus;
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
        .with_env_var("SNOT_LOG")
        .from_env_lossy()
        .add_directive("surrealdb_core=off".parse().unwrap())
        .add_directive("surrealdb=off".parse().unwrap())
        .add_directive("tungstenite=off".parse().unwrap())
        .add_directive("tokio_tungstenite=off".parse().unwrap())
        .add_directive("tokio_util=off".parse().unwrap())
        .add_directive("bollard=ERROR".parse().unwrap())
        .add_directive("tarpc::client=ERROR".parse().unwrap())
        .add_directive("tarpc::server=ERROR".parse().unwrap())
        .add_directive("tower_http::trace::on_request=off".parse().unwrap())
        .add_directive("tower_http::trace::on_response=off".parse().unwrap());

    dbg!(env_filter.to_string());

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

    let mut path = cli.path.clone();
    path.push("data.db");

    let db = Surreal::new::<surrealdb::engine::local::File>(path)
        .await
        .expect("failed to create surrealDB");

    let state = GlobalState {
        cli,
        db,
        prom_ctr: Default::default(),
        pool: Default::default(),
        storage_ids: Default::default(),
        storage: Default::default(),
        envs: Default::default(),
    };

    prometheus::init(&state)
        .await
        .expect("failed to launch prometheus container");

    server::start(Arc::new(state)).await.expect("start server");
}
