use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use snot_common::prelude::*;
use tarpc::context;
use tokio::select;
use tracing::info;

use crate::state::{Agent, GlobalState};

mod api;
mod content;

type AppState = Arc<GlobalState>;

pub async fn start() -> Result<()> {
    let state = GlobalState::default();

    let app = Router::new()
        .route("/agent", get(agent_ws_handler))
        .nest("/api/v1", api::routes()) // TODO: authorization
        .nest("/content", content::routes())
        .with_state(Arc::new(state));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:1234").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn agent_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
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

    // set up the RPC client
    let (client_response_in, client_transport, mut client_request_out) = RpcTransport::new();
    let client =
        AgentServiceClient::new(tarpc::client::Config::default(), client_transport).spawn();

    let agent = Agent::new(client);
    let id = agent.id();

    let test_client = agent.client();
    tokio::spawn(async move {
        test_client
            .reconcile(AgentState::Inventory)
            .await
            .expect("reconcilation");
    });

    // register the agent with the agent pool
    {
        let mut pool = state.pool.write().await;
        pool.insert(id, agent);
        info!(
            "new agent connected (id {id}); pool is now {} nodes",
            pool.len()
        );
    }

    loop {
        // TODO: multiplex for bi-directional RPC
        select! {
            // handle incoming messages
            msg = socket.recv() => {
                match msg {
                    Some(Err(_)) | None => break,
                    Some(Ok(Message::Binary(bin))) => {
                        let msg = bincode::deserialize(&bin).expect("deserialize incoming message");
                        client_response_in.send(msg).expect("internal RPC channel closed");
                    }
                    _ => (),
                }
            }

            // handle outgoing messages
            msg = client_request_out.recv() => {
                let msg = msg.expect("internal RPC channel closed");
                let bin = bincode::serialize(&msg).expect("failed to serialize request");
                if let Err(_) = socket.send(Message::Binary(bin)).await {
                    break;
                }
            }
        }
    }

    // remove the node from the node pool
    {
        let mut pool = state.pool.write().await;
        pool.remove(&id);
        info!("agent {id} disconnected; pool is now {} nodes", pool.len());
    }
}
