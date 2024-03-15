use std::sync::Arc;

use anyhow::Result;
use axum::{extract::State, response::IntoResponse, routing::get, Router};
use axum_typed_websockets::{Codec, Message, WebSocket, WebSocketUpgrade};
use serde::{de::DeserializeOwned, Serialize};
use snot_common::prelude::*;
use tokio::sync::mpsc;

use crate::state::{Agent, GlobalState};

mod routes;

type Socket = WebSocket<ServerMessage, ClientMessage, BinaryCodec>;
type SocketUpgrade = WebSocketUpgrade<ServerMessage, ClientMessage, BinaryCodec>;

type AppState = Arc<GlobalState>;

pub async fn start() -> Result<()> {
    let state = GlobalState::default();

    let app = Router::new()
        .route("/agent", get(agent_ws_handler))
        .nest("/api", routes::api()) // TODO: authorization
        // TODO: file serving (binaries, ledger, blocks, etc.)
        .with_state(Arc::new(state));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:1234").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn agent_ws_handler(ws: SocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: Socket, state: AppState) {
    // TODO
    // the server will add all known nodes to a "pool" of nodes. the server can then
    // dynamically assign a node to be whatever type it needs to be when the desired
    // state changes.
    //
    // when a test is started on the server, it will associate each `nodes` (from
    // the test definition) with a particular node in the node pool. the server will
    // tell the node what state it now expects it to have (for example, telling it
    // that it is an offline validator with some ledger and some block height), and
    // the agent will synchronize with that state. that is, the server doesn't
    // necessarily tell the agents to do *something*, the server just shows the
    // agents a desired final state and the agents work to reach that final state by
    // starting/stopping snarkOS and altering the local ledger.

    // TODO: the client should provide us with some information about itself (num
    // cpus, etc.) before we categorize it and add it as an agent to the agent pool

    let (tx, mut rx) = mpsc::channel(10 /* ? */);

    let agent = Agent::new(tx);
    let id = agent.id();

    // register the agent with the agent pool
    state.pool.write().await.insert(id, agent);

    while let Some(message) = rx.recv().await {
        if let Err(_) = socket.send(Message::Item(message)).await {
            break;
        }
    }

    // remove the node from the node pool
    state.pool.write().await.remove(&id);
}

struct BinaryCodec;

impl Codec for BinaryCodec {
    type Error = bincode::Error;

    fn decode<R: DeserializeOwned>(buf: Vec<u8>) -> Result<R, Self::Error> {
        bincode::deserialize(&buf)
    }

    fn encode<S: Serialize>(msg: S) -> Result<Vec<u8>, Self::Error> {
        bincode::serialize(&msg)
    }
}
