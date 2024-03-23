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
    extract::{self, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Args;
use reqwest::StatusCode;
use serde_json::json;
use tracing_appender::non_blocking::NonBlocking;

use crate::{Block, DbLedger, Transaction};

#[derive(Debug, Args, Clone)]
/// Receive inquiries on /mainnet/latest/stateRoot
pub struct LedgerQuery {
    #[arg(long, default_value = "3030")]
    /// Port to listen on for incoming messages
    pub port: u16,

    #[arg(long, default_value = "0.0.0.0")]
    // IP address to bind to
    pub bind: IpAddr,

    #[arg(long)]
    /// When true, the POST /block endpoint will not be available
    pub readonly: bool,

    #[arg(long)]
    /// Receive messages from /mainnet/transaction/broadcast and record them to the output
    pub record: bool,

    #[arg(long, short, default_value = "transactions.json")]
    /// Path to the directory containing the stored data
    pub output: PathBuf,
}

struct LedgerState {
    readonly: bool,
    ledger: DbLedger,
    appender: Option<NonBlocking>,
}

type AppState = Arc<LedgerState>;

impl LedgerQuery {
    #[tokio::main]
    pub async fn parse(self, ledger: &DbLedger) -> Result<()> {
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
        };

        let app = Router::new()
            .route("/mainnet/latest/stateRoot", get(Self::latest_state_root))
            .route("/mainnet/block/height/latest", get(Self::latest_height))
            .route("/mainnet/block/hash/latest", get(Self::latest_hash))
            .route("/mainnet/transaction/broadcast", post(Self::broadcast_tx))
            .route("/block", post(Self::add_block))
            .with_state(Arc::new(state));

        let listener = tokio::net::TcpListener::bind(SocketAddr::new(self.bind, self.port)).await?;
        tracing::info!("listening on: {:?}", listener.local_addr().unwrap());
        axum::serve(listener, app).await?;

        Ok(())
    }

    async fn latest_state_root(state: State<AppState>) -> impl IntoResponse {
        Json(json!(state.ledger.latest_state_root()))
    }

    async fn latest_height(state: State<AppState>) -> impl IntoResponse {
        Json(json!(state.ledger.latest_height()))
    }

    async fn latest_hash(state: State<AppState>) -> impl IntoResponse {
        Json(json!(state.ledger.latest_hash()))
    }

    async fn broadcast_tx(
        state: State<AppState>,
        payload: extract::Json<Transaction>,
    ) -> impl IntoResponse {
        let Ok(tx_json) = serde_json::to_string(payload.deref()) else {
            return StatusCode::BAD_REQUEST;
        };

        if let Some(mut a) = state.appender.clone() {
            match write!(a, "{}", tx_json) {
                Ok(_) => StatusCode::OK,
                Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
            }
        } else {
            println!("{}", tx_json);
            StatusCode::OK
        }
    }

    async fn add_block(state: State<AppState>, payload: extract::Json<Block>) -> impl IntoResponse {
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
}
