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
use futures_util::stream::StreamExt;
use snot_common::prelude::*;
use tarpc::server::Channel;
use tokio::select;
use tracing::{info, warn};

use self::rpc::ControlRpcServer;
use crate::{
    server::rpc::{MuxedMessageIncoming, MuxedMessageOutgoing},
    state::{Agent, GlobalState},
};

mod api;
mod content;
mod rpc;

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
    // TODO: the handshake should include a JWT with the known agent ID, or the
    // control plane should prescribe the agent with a new ID and JWT

    // TODO: the client should provide us with some information about itself (num
    // cpus, etc.) before we categorize it and add it as an agent to the agent pool

    // set up the RPC channels
    let (client_response_in, client_transport, mut client_request_out) = RpcTransport::new();
    let (server_request_in, server_transport, mut server_response_out) = RpcTransport::new();

    // set up the client, facing the agent server
    let client =
        AgentServiceClient::new(tarpc::client::Config::default(), client_transport).spawn();

    let agent = Agent::new(client);
    let id = agent.id();

    // set up the server, for incoming RPC requests
    let server = tarpc::server::BaseChannel::with_defaults(server_transport);
    let server_handle = tokio::spawn(
        server
            .execute(
                ControlRpcServer {
                    state: state.to_owned(),
                    agent: id,
                }
                .serve(),
            )
            .for_each(|r| async move {
                tokio::spawn(r);
            }),
    );

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
        select! {
            // handle incoming messages
            msg = socket.recv() => {
                match msg {
                    Some(Err(_)) | None => break,
                    Some(Ok(Message::Binary(bin))) => {
                        let msg = match bincode::deserialize(&bin) {
                            Ok(msg) => msg,
                            Err(e) => {
                                warn!("failed to deserialize a message from agent {id}: {e}");
                                continue;
                            }
                        };

                        match msg {
                            MuxedMessageIncoming::Control(msg) => server_request_in.send(msg).expect("internal RPC channel closed"),
                            MuxedMessageIncoming::Agent(msg) => client_response_in.send(msg).expect("internal RPC channel closed"),
                        }
                    }
                    _ => (),
                }
            }

            // handle outgoing requests
            msg = client_request_out.recv() => {
                let msg = msg.expect("internal RPC channel closed");
                let bin = bincode::serialize(&MuxedMessageOutgoing::Agent(msg)).expect("failed to serialize request");
                if let Err(_) = socket.send(Message::Binary(bin)).await {
                    break;
                }
            }

            // handle outgoing responses
            msg = server_response_out.recv() => {
                let msg = msg.expect("internal RPC channel closed");
                let bin = bincode::serialize(&MuxedMessageOutgoing::Control(msg)).expect("failed to serialize response");
                if let Err(_) = socket.send(Message::Binary(bin)).await {
                    break;
                }
            }
        }
    }

    // abort the RPC server handle
    server_handle.abort();

    // remove the node from the node pool
    {
        let mut pool = state.pool.write().await;
        pool.remove(&id);
        info!("agent {id} disconnected; pool is now {} nodes", pool.len());
    }
}
