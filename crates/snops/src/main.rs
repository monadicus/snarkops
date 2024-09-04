use std::{io, net::SocketAddr, sync::Arc};

use clap::Parser;
use cli::Cli;
use prometheus_http_query::Client as PrometheusClient;
use schema::storage::{DEFAULT_AGENT_BINARY, DEFAULT_AOT_BINARY};
use snops_common::db::Database;
use state::GlobalState;
use tokio::select;
use tracing::{error, info, level_filters::LevelFilter, trace};
use tracing_subscriber::{prelude::*, reload, EnvFilter};

pub mod cannon;
pub mod cli;
pub mod db;
pub mod env;
pub mod error;
pub mod logging;
pub mod persist;
pub mod schema;
pub mod server;
pub mod state;

type ReloadHandler = reload::Handle<EnvFilter, tracing_subscriber::Registry>;

fn make_env_filter(level: LevelFilter) -> EnvFilter {
    EnvFilter::builder()
        .with_env_var("SNOPS_LOG")
        .with_default_directive(level.into())
        .from_env_lossy()
        .add_directive("hyper_util=off".parse().unwrap())
        .add_directive("hyper=off".parse().unwrap())
        .add_directive("reqwest=off".parse().unwrap())
        .add_directive("tungstenite=off".parse().unwrap())
        .add_directive("tokio_tungstenite=off".parse().unwrap())
        .add_directive("tarpc::client=ERROR".parse().unwrap())
        .add_directive("tarpc::server=ERROR".parse().unwrap())
        .add_directive("tower_http::trace::on_request=off".parse().unwrap())
        .add_directive("tower_http::trace::on_response=off".parse().unwrap())
}

#[tokio::main]
async fn main() {
    let filter_level = if cfg!(debug_assertions) {
        LevelFilter::TRACE
    } else {
        LevelFilter::INFO
    };

    let (env_filter, reload_handler) = reload::Layer::new(make_env_filter(filter_level));
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

    info!("Using AOT binary:\n{}", DEFAULT_AOT_BINARY.to_string());
    info!("Using Agent binary:\n{}", DEFAULT_AGENT_BINARY.to_string());

    trace!("Loading prometheus client");
    let prometheus = cli
        .prometheus
        .as_ref()
        .and_then(|p| PrometheusClient::try_from(p.as_str()).ok());

    trace!("Creating store");
    let db = db::Database::open(&cli.path.join("store")).expect("open database");
    let socket_addr = SocketAddr::new(cli.bind_addr, cli.port);

    trace!("Loading state");
    let state = GlobalState::load(cli, db, prometheus, reload_handler)
        .await
        .expect("load state");

    // start the task that manages external peer block status
    let info_task = tokio::spawn(state::external_peers::block_info_task(Arc::clone(&state)));
    // start the task that manages transaction tracking status
    let transaction_task = tokio::spawn(state::transactions::tracking_task(Arc::clone(&state)));
    // start the task that manages cache invalidation
    let cache_task = tokio::spawn(env::cache::invalidation_task(Arc::clone(&state)));

    info!("Starting server on {socket_addr}");
    select! {
        Err(err) = server::start(Arc::clone(&state), socket_addr) => {
            error!("error starting server: {err:?}");
        }
        Err(err) = info_task => {
            error!("block info task failed: {err:?}");
        }
        Err(err) = transaction_task => {
            error!("transaction task failed: {err:?}");
        }
        Err(err) = cache_task => {
            error!("cache invalidation task failed: {err:?}");
        }
    }
}
