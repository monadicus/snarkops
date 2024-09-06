use std::{fs::File, io};

use clap::Parser;
use clap_serde_derive::ClapSerde;
use cli::{Cli, Config};
use schema::storage::{DEFAULT_AGENT_BINARY, DEFAULT_AOT_BINARY};
use snops_common::util::parse_file_from_extension;
use tracing::{info, level_filters::LevelFilter};
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
    let mut cli = Cli::parse();

    // build a full config object from CLI and from optional config file
    let config = 'config: {
        match cli.config_path {
            // file passed: try to open it
            Some(path) => match File::open(&path) {
                // file opened: try to parse it
                Ok(file) => {
                    match parse_file_from_extension::<<Config as ClapSerde>::Opt>(&path, file) {
                        // merge file with CLI args
                        Ok(config) => break 'config Config::from(config).merge(&mut cli.config),

                        // file parse fail
                        Err(e) => {
                            tracing::warn!("failed to parse config file, ignoring. error: {e}")
                        }
                    }
                }

                // file open fail
                Err(e) => tracing::warn!("failed to open config file, ignoring. error: {e}"),
            },

            // no file
            None => (),
        };

        Config::from(&mut cli.config)
    };

    info!("Using AOT binary:\n{}", DEFAULT_AOT_BINARY.to_string());
    info!("Using Agent binary:\n{}", DEFAULT_AGENT_BINARY.to_string());

    server::start(config, reload_handler)
        .await
        .expect("start server");
}
