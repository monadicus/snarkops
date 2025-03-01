use std::{
    fs::File,
    io::Write,
    net::{IpAddr, SocketAddr},
    ops::Deref,
    path::PathBuf,
    sync::Arc,
};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{self, Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use clap::Args;
use reqwest::StatusCode;
use serde_json::json;
use tracing_appender::non_blocking::NonBlocking;

use crate::{
    Block, DbLedger, Network, Transaction,
    cli::{ReloadHandler, make_env_filter},
};

/// Receive inquiries on `/<network>/latest/stateRoot`.
#[derive(Debug, Args, Clone)]
pub struct LedgerQuery<N: Network> {
    /// Port to listen on for incoming messages.
    #[arg(long, default_value = "3030")]
    pub port: u16,

    // IP address to bind to.
    #[arg(long, default_value = "0.0.0.0")]
    pub bind: IpAddr,

    /// When true, the POST `/block` endpoint will not be available.
    #[arg(long)]
    pub readonly: bool,

    /// Receive messages from `/<network>/transaction/broadcast` and record them
    /// to the output.
    #[arg(long)]
    pub record: bool,

    /// Path to the directory containing the stored data.
    #[arg(long, short, default_value = "transactions.json")]
    pub output: PathBuf,

    #[clap(skip)]
    phantom: std::marker::PhantomData<N>,
}

struct LedgerState<N: Network> {
    readonly: bool,
    ledger: DbLedger<N>,
    appender: Option<NonBlocking>,
    log_level_handler: ReloadHandler,
}

type AppState<N> = Arc<LedgerState<N>>;

impl<N: Network> LedgerQuery<N> {
    #[tokio::main]
    pub async fn parse(self, ledger: &DbLedger<N>, log_level_handler: ReloadHandler) -> Result<()> {
        let (appender, _guard) = if self.record {
            let (appender, guard) = tracing_appender::non_blocking(
                File::options()
                    .create(true)
                    .append(true)
                    .open(self.output.clone())
                    .expect("Failed to open the file for writing transactions"),
            );
            (Some(appender), Some(guard))
        } else {
            (None, None)
        };

        let state = LedgerState {
            readonly: self.readonly,
            ledger: ledger.clone(),
            appender,
            log_level_handler,
        };

        let network = N::str_id();

        let app = Router::new()
            .route(
                &format!("/{network}/latest/stateRoot"),
                get(Self::latest_state_root),
            )
            .route(
                &format!("/{network}/stateRoot/latest"),
                get(Self::latest_state_root),
            )
            .route(
                &format!("/{network}/block/height/latest"),
                get(Self::latest_height),
            )
            .route(
                &format!("/{network}/block/hash/latest"),
                get(Self::latest_hash),
            )
            .route(
                &format!("/{network}/transaction/broadcast"),
                post(Self::broadcast_tx),
            )
            .route("/block", post(Self::add_block))
            .route("/log", post(Self::set_log_level))
            // TODO: for ahead of time ledger generation, support a /beacon_block endpoint to write
            // beacon block TODO: api to get and decrypt records for a private key
            .with_state(Arc::new(state));

        let listener = tokio::net::TcpListener::bind(SocketAddr::new(self.bind, self.port)).await?;
        tracing::info!("listening on: {:?}", listener.local_addr().unwrap());
        axum::serve(listener, app).await?;

        Ok(())
    }

    async fn latest_state_root(state: State<AppState<N>>) -> impl IntoResponse {
        Json(json!(state.ledger.latest_state_root()))
    }

    async fn latest_height(state: State<AppState<N>>) -> impl IntoResponse {
        Json(json!(state.ledger.latest_height()))
    }

    async fn latest_hash(state: State<AppState<N>>) -> impl IntoResponse {
        Json(json!(state.ledger.latest_hash()))
    }

    async fn broadcast_tx(
        state: State<AppState<N>>,
        payload: extract::Json<Transaction<N>>,
    ) -> impl IntoResponse {
        let Ok(tx_json) = serde_json::to_string(payload.deref()) else {
            return StatusCode::BAD_REQUEST;
        };

        match state.appender.clone() {
            Some(mut a) => match write!(a, "{}", tx_json) {
                Ok(_) => StatusCode::OK,
                Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
            },
            _ => {
                println!("{}", tx_json);
                StatusCode::OK
            }
        }
    }

    async fn add_block(
        state: State<AppState<N>>,
        payload: extract::Json<Block<N>>,
    ) -> impl IntoResponse {
        if state.readonly {
            return (StatusCode::FORBIDDEN, Json(json!({"error": "readonly"})));
        }

        if state.ledger.latest_hash() != payload.previous_hash()
            || state.ledger.latest_state_root() != payload.previous_state_root()
            || state.ledger.latest_height() + 1 != payload.height()
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid block"})),
            );
        }

        if let Err(e) = state
            .ledger
            .check_next_block(&payload, &mut rand::thread_rng())
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("failed to validate block: {e}")})),
            );
        }

        match state.ledger.advance_to_next_block(&payload) {
            Ok(_) => (StatusCode::OK, Json(json!({"status": "ok"}))),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("failed to advance block: {e}")})),
            ),
        }
    }

    async fn set_log_level(
        state: State<AppState<N>>,
        Query(verbosity): Query<u8>,
    ) -> impl IntoResponse {
        let Ok(_) = state
            .log_level_handler
            .modify(|filter| *filter = make_env_filter(verbosity))
        else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "failed to set log level"})),
            );
        };

        (StatusCode::OK, Json(json!({"status": "ok"})))
    }
}
