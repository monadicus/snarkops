use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use futures::{SinkExt, StreamExt};
use http::{HeaderValue, Uri};
use snops_common::{
    constant::{ENV_AGENT_KEY, HEADER_AGENT_KEY},
    rpc::{
        control::{agent::AgentService, ControlServiceClient, PING_HEADER},
        RpcTransport, PING_INTERVAL_SEC, PING_LENGTH,
    },
};
use tarpc::server::Channel;
use tokio::select;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, client::IntoClientRequest, handshake::client::Request},
};
use tracing::{error, info, warn};

use crate::{
    rpc::control::{self, AgentRpcServer},
    state::GlobalState,
};

pub fn new_ws_request(ws_uri: &Uri, jwt: Option<String>) -> Request {
    let mut req = ws_uri.to_owned().into_client_request().unwrap();

    // attach JWT if we have one
    if let Some(jwt) = jwt {
        req.headers_mut().insert(
            "Authorization",
            HeaderValue::from_bytes(format!("Bearer {jwt}").as_bytes())
                .expect("attach authorization header"),
        );
    }

    // attach agent key if one is set in env vars
    if let Ok(key) = std::env::var(ENV_AGENT_KEY) {
        req.headers_mut().insert(
            HEADER_AGENT_KEY,
            HeaderValue::from_bytes(key.as_bytes()).expect("attach agent key header"),
        );
    }

    req
}

pub async fn ws_connection(ws_req: Request, state: Arc<GlobalState>) {
    let (mut stream, _response) = match connect_async(ws_req).await {
        Ok(res) => res,
        Err(e) => {
            error!("failed to connect to websocket: {e}");
            return;
        }
    };

    info!("Connection established with the control plane");

    // Clear old info cache. we will get new info from the control plane
    state.set_env_info(None).await;
    // TODO: fetch latest info from controlplane rather than clearing

    // create rpc channels
    let (client_response_in, client_transport, mut client_request_out) = RpcTransport::new();
    let (server_request_in, server_transport, mut server_response_out) = RpcTransport::new();

    // set up the client, facing the control plane
    let client =
        ControlServiceClient::new(tarpc::client::Config::default(), client_transport).spawn();
    state.client.write().await.replace(client.clone());

    let start_time = Instant::now();
    let mut interval = tokio::time::interval(Duration::from_secs(PING_INTERVAL_SEC));
    let mut num_pings: u32 = 0;

    // initialize and start the rpc server
    let mut server_handle = Box::pin(
        tarpc::server::BaseChannel::with_defaults(server_transport).execute(
            AgentRpcServer {
                client,
                state: Arc::clone(&state),
                version: env!("CARGO_PKG_VERSION"),
            }
            .serve(),
        ),
    );

    loop {
        select! {
            _ = interval.tick() => {
                // ping payload contains "snops-agent", number of pings, and uptime
                let mut payload = Vec::from(PING_HEADER);
                payload.extend_from_slice(&num_pings.to_le_bytes());
                payload.extend_from_slice(&start_time.elapsed().as_micros().to_le_bytes());

                let send = stream.send(tungstenite::Message::Ping(payload));
                if tokio::time::timeout(Duration::from_secs(10), send).await.is_err() {
                    error!("The connection to the control plane was interrupted while sending ping");
                    break
                }
            }

            // handle outgoing responses
            msg = server_response_out.recv() => {
                let Some(msg) = msg else {
                    error!("internal agent RPC channel closed");
                    break;
                };
                let bin = match bincode::serialize(&control::MuxedMessageOutgoing::Child(msg)) {
                    Ok(bin) => bin,
                    Err(e) => {
                        error!("failed to serialize response: {e}");
                        continue;
                    }
                };

                let send = stream.send(tungstenite::Message::Binary(bin));
                if tokio::time::timeout(Duration::from_secs(10), send).await.is_err() {
                    error!("The connection to the control plane was interrupted while sending agent message");
                    break;
                }
            }

            // handle outgoing requests
            msg = client_request_out.recv() => {
                let Some(msg) = msg else {
                    error!("internal agent RPC channel closed");
                    break;
                };
                let bin = match bincode::serialize(&control::MuxedMessageOutgoing::Parent(msg)) {
                    Ok(bin) => bin,
                    Err(e) => {
                        error!("failed to serialize request: {e}");
                        continue;
                    }
                };
                let send = stream.send(tungstenite::Message::Binary(bin));
                if tokio::time::timeout(Duration::from_secs(10), send).await.is_err() {
                    error!("The connection to the control plane was interrupted while sending control message");
                    break;
                }
            }

            // handle incoming messages
            msg = stream.next() => match msg {
                Some(Ok(tungstenite::Message::Close(frame))) => {
                    if let Some(frame) = frame {
                        info!("The control plane has closed the connection: {frame}");
                    } else {
                        info!("The control plane has closed the connection");
                    }
                    break;
                }

                Some(Ok(tungstenite::Message::Pong(payload))) => {
                    let mut payload = payload.as_slice();
                    // check the header
                    if !payload.starts_with(PING_HEADER) {
                        warn!("Received a pong payload with an invalid header prefix");
                        continue;
                    }
                    payload = &payload[PING_HEADER.len()..];
                    if payload.len() != PING_LENGTH {
                        warn!("Received a pong payload with an invalid length {}, expected {PING_LENGTH}", payload.len());
                        continue;
                    }
                    let (left, right) = payload.split_at(size_of::<u32>());
                    let ping_index = u32::from_le_bytes(left.try_into().unwrap());
                    let _uptime_start = u128::from_le_bytes(right.try_into().unwrap());

                    if ping_index != num_pings {
                        warn!("Received a pong payload with an invalid index {ping_index}, expected {num_pings}");
                        continue;
                    }

                    num_pings += 1;

                    // when desired, we can add this as a metric
                    // let uptime_now = start_time.elapsed().as_micros();
                    // let uptime_diff = uptime_now - uptime_start;
                }

                Some(Ok(tungstenite::Message::Binary(bin))) => {
                    let msg = match bincode::deserialize(&bin) {
                        Ok(msg) => msg,
                        Err(e) => {
                            error!("failed to deserialize a message from the control plane: {e}");
                            continue;
                        }
                    };

                    match msg {
                        control::MuxedMessageIncoming::Child(msg) => {
                            if let Err(e) = server_request_in.send(msg) {
                                error!("internal agent RPC channel closed: {e}");
                                break;
                            }
                        },
                        control::MuxedMessageIncoming::Parent(msg) => {
                            if let Err(e) = client_response_in.send(msg) {
                                error!("internal agent RPC channel closed: {e}");
                                break;
                            }
                        }
                    }
                }

                None | Some(Err(_)) => {
                    error!("The connection to the control plane was interrupted");
                    break;
                }

                Some(Ok(o)) => println!("{o:#?}"),
            },

            // handle server requests
            Some(r) = server_handle.next() => {
                tokio::spawn(r);
            }
        }
    }
}
