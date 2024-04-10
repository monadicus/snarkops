use std::{io, sync::Arc};

use clap::Parser;
use cli::Cli;
use server::error::StartError;
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};
use state::{util::OpaqueDebug, GlobalState};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;

pub mod cannon;
pub mod cli;
pub mod env;
pub mod error;
pub mod logging;
pub mod models;
pub mod schema;
pub mod server;
pub mod state;

#[tokio::main]
async fn main() -> Result<(), StartError> {
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
        .add_directive("sqlx=off".parse().unwrap())
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

    let cli = Cli::parse();

    let mut path = cli.path.clone();
    path.push("data.db");
    let db_path = path.as_os_str().to_str().unwrap();

    if !Sqlite::database_exists(db_path).await.unwrap_or(false) {
        match Sqlite::create_database(db_path).await {
            Ok(_) => println!("Create db success"),
            Err(error) => todo!("error: {}", error),
        }
    }

    let db = SqlitePool::connect(db_path)
        .await
        .map_err(StartError::DbConnect)?;

    let prometheus = cli
        .prometheus
        .and_then(|p| prometheus_http_query::Client::try_from(format!("http://{p}")).ok()); // TODO: https

    let state = GlobalState {
        cli,
        db,
        pool: Default::default(),
        storage_ids: Default::default(),
        storage: Default::default(),
        envs: Default::default(),
        prom_httpsd: Default::default(),
        prometheus: OpaqueDebug(prometheus),
    };

    server::start(Arc::new(state)).await.expect("start server");
    Ok(())
}
