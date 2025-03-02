use std::sync::Arc;

use anyhow::Result;
use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::{IntoResponse, Response},
    routing::get,
};
use futures::StreamExt;
use snops_common::rpc::{
    RpcTransport,
    agent::{AgentNodeService, node::NodeServiceClient},
};
use tarpc::server::Channel;
use tokio::select;
use tracing::{error, info, warn};

use crate::{
    rpc::agent::{AgentNodeRpcServer, MuxedMessageIncoming, MuxedMessageOutgoing},
    state::AppState,
};

pub async fn start(listener: tokio::net::TcpListener, state: AppState) -> Result<()> {
    let app = Router::new()
        .route("/node", get(node_ws_handler))
        .with_state(Arc::clone(&state));
    info!(
        "Starting internal node RPC server on: {}",
        listener.local_addr()?
    );

    axum::serve(listener, app).await?;

    Ok(())
}

async fn node_ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
        .into_response()
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut node_client = state.node_client.write().await;
    if node_client.is_some() {
        warn!("A new node RPC connection tried to establish when one was already established");
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    // set up the RPC channels
    let (client_response_in, client_transport, mut client_request_out) = RpcTransport::new();
    let (server_request_in, server_transport, mut server_response_out) = RpcTransport::new();

    // set up the client, facing the node server
    let client = NodeServiceClient::new(tarpc::client::Config::default(), client_transport).spawn();

    // store the client in state
    tracing::info!("Connection established with the node");
    *node_client = Some(client);
    drop(node_client);

    // set up the server for incoming RPC requests
    let server = tarpc::server::BaseChannel::with_defaults(server_transport);
    let server_handle = tokio::spawn(
        server
            .execute(
                AgentNodeRpcServer {
                    state: Arc::clone(&state),
                }
                .serve(),
            )
            .for_each(|r| async move {
                tokio::spawn(r);
            }),
    );

    loop {
        select! {
            // handle incoming messages
            msg = socket.recv() => {
                match msg {
                    Some(Err(_)) | None => break,
                    Some(Ok(Message::Binary(bin))) => {
                        let msg = match snops_common::rpc::codec::decode(&bin) {
                            Ok(msg) => msg,
                            Err(e) => {
                                error!("failed to deserialize a message from node: {e}");
                                continue;
                            }
                        };

                        match msg {
                            MuxedMessageIncoming::Parent(msg) => {
                                if let Err(e) = server_request_in.send(msg) {
                                    error!("internal node RPC channel closed: {e}");
                                    break;
                                }
                            },
                            MuxedMessageIncoming::Child(msg) => {
                                if let Err(e) = client_response_in.send(msg) {
                                    error!("internal node RPC channel closed: {e}");
                                    break;
                                }
                            }
                        }
                    }
                    _ => (),
                }
            }

            // handle outgoing requests
            msg = client_request_out.recv() => {
                let Some(msg) = msg else { error!("internal node RPC channel closed"); break; };
                let bin = match snops_common::rpc::codec::encode(&MuxedMessageOutgoing::Child(msg)) {
                    Ok(bin) => bin,
                    Err(e) => {
                        error!("failed to serialize a request to node: {e}");
                        continue;
                    }
                };
                if socket.send(Message::Binary(bin)).await.is_err() {
                    break;
                }
            }

            // handle outgoing response
            msg = server_response_out.recv() => {
                let Some(msg) = msg else { error!("internal node RPC channel closed"); break; };
                let bin = match snops_common::rpc::codec::encode(&MuxedMessageOutgoing::Parent(msg)) {
                    Ok(bin) => bin,
                    Err(e) => {
                        error!("failed to serialize a response to node: {e}");
                        continue;
                    }
                };
                if socket.send(Message::Binary(bin)).await.is_err() {
                    break;
                }
            }
        }
    }

    // abort the RPC server handle
    server_handle.abort();
    state.node_client.write().await.take();
}
