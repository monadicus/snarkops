use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use http::Uri;
use node::{MuxedMessageIncoming, MuxedMessageOutgoing, NodeRpcServer};
use snarkvm::{
    ledger::store::{helpers::rocksdb::BlockDB, BlockStorage},
    prelude::Network,
};
use snops_common::{
    rpc::{
        agent::{node::NodeService, AgentNodeServiceClient, PING_HEADER},
        RpcTransport, PING_INTERVAL_SEC, PING_LENGTH,
    },
    state::snarkos_status::{SnarkOSBlockInfo, SnarkOSStatus},
};
use tarpc::{context, server::Channel};
use tokio::select;
use tokio_tungstenite::{connect_async, tungstenite, tungstenite::client::IntoClientRequest};
use tracing::{error, info, warn};

pub mod node;

#[derive(Clone)]
pub enum RpcClient {
    Enabled {
        port: u16,
        client: AgentNodeServiceClient,
    },
    Disabled,
}

impl RpcClient {
    pub fn new(port: Option<u16>) -> Self {
        let Some(port) = port else {
            return Self::Disabled;
        };

        let start_time = Instant::now();

        // create RPC channels
        let (client_response_in, client_transport, mut client_request_out) = RpcTransport::new();
        let (server_request_in, server_transport, mut server_response_out) = RpcTransport::new();

        // set up the client, facing the agent
        let client =
            AgentNodeServiceClient::new(tarpc::client::Config::default(), client_transport).spawn();

        // initialize and start the RPC server
        let server = tarpc::server::BaseChannel::with_defaults(server_transport);
        tokio::spawn(
            server
                .execute(NodeRpcServer { state: () }.serve())
                .for_each(|r| async move {
                    tokio::spawn(r);
                }),
        );

        // get a URL/req that can be used to connect to the agent
        let ws_req = Uri::builder()
            .scheme("http")
            .authority("127.0.0.1")
            .path_and_query("/node")
            .build()
            .expect("failed to build websocket URL")
            .into_client_request()
            .unwrap();

        // ws connection loop
        tokio::spawn(async move {
            loop {
                'connection: {
                    let (mut ws_stream, _) = match connect_async(ws_req.to_owned()).await {
                        Ok(r) => r,
                        Err(e) => {
                            error!("An error occurred establishing the connection: {e}");
                            break 'connection;
                        }
                    };

                    let mut interval =
                        tokio::time::interval(Duration::from_secs(PING_INTERVAL_SEC));
                    let mut num_pings: u32 = 0;

                    'event: loop {
                        select! {
                            // ping
                            _ = interval.tick() => {
                                let mut payload = Vec::from(PING_HEADER);
                                payload.extend_from_slice(&num_pings.to_le_bytes());
                                payload.extend_from_slice(&start_time.elapsed().as_micros().to_le_bytes());

                                let send = ws_stream.send(tungstenite::Message::Ping(payload));
                                if tokio::time::timeout(Duration::from_secs(10), send).await.is_err() {
                                    error!("The connection to the control plane was interrupted while sending ping");
                                    break 'event;
                                }
                            }

                            // handle outgoing responses
                            msg = server_response_out.recv() => {
                                let msg = msg.expect("internal RPC channel closed");
                                let bin = bincode::serialize(&MuxedMessageOutgoing::Child(msg)).expect("failed to serialize response");
                                let send = ws_stream.send(tungstenite::Message::Binary(bin));
                                if tokio::time::timeout(Duration::from_secs(10), send).await.is_err() {
                                    error!("The connection to the agent was interrupted while sending node message");
                                    break 'event;
                                }
                            }

                            // handle outgoing requests
                            msg = client_request_out.recv() => {
                                let msg = msg.expect("internal RPC channel closed");
                                let bin = bincode::serialize(&MuxedMessageOutgoing::Parent(msg)).expect("failed to serialize request");
                                let send = ws_stream.send(tungstenite::Message::Binary(bin));
                                if tokio::time::timeout(Duration::from_secs(10), send).await.is_err() {
                                    error!("The connection to the agent was interrupted while sending node message");
                                    break 'event;
                                }
                            }

                            // handle incoming messages
                            msg = ws_stream.next() => match msg {
                                Some(Ok(tungstenite::Message::Close(frame))) => {
                                    match frame {
                                        Some(frame) => info!("The agent closed the connection: {frame}"),
                                        None => info!("The agent closed the connection"),
                                    }
                                    break 'event;
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
                                }

                                Some(Ok(tungstenite::Message::Binary(bin))) => {
                                    let msg = match bincode::deserialize(&bin) {
                                        Ok(msg) => msg,
                                        Err(e) => {
                                            error!("failed to deserialize a message from the agent: {e}");
                                            continue;
                                        }
                                    };

                                    match msg {
                                        MuxedMessageIncoming::Child(msg) => server_request_in.send(msg).expect("internal RPC channel closed"),
                                        MuxedMessageIncoming::Parent(msg) => client_response_in.send(msg).expect("internal RPC channel closed"),
                                    }
                                }

                                None | Some(Err(_)) => {
                                    error!("The connection to the agent was interrupted");
                                    break 'event;
                                }

                                Some(Ok(o)) => println!("{o:#?}"),
                            }
                        }
                    }
                }
            }
        });

        Self::Enabled { port, client }
    }
}

impl RpcClient {
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled { .. })
    }

    pub fn status(&self, body: SnarkOSStatus) {
        if let Self::Enabled { client, .. } = self.to_owned() {
            tokio::spawn(async move {
                let _ = client.post_status(context::current(), body).await;
            });
        }
    }

    pub fn post_block<N: Network>(&self, height: u32, blocks: &BlockDB<N>) {
        if let Self::Enabled { client, .. } = self.to_owned() {
            // lookup block hash and state root
            let (Ok(Some(block_hash)), Ok(Some(state_root))) =
                (blocks.get_block_hash(height), blocks.get_state_root(height))
            else {
                return;
            };

            // lookup block header
            let Ok(Some(header)) = blocks.get_block_header(&block_hash) else {
                return;
            };

            // assemble the body
            let body = SnarkOSBlockInfo {
                height,
                state_root: state_root.to_string(),
                block_hash: block_hash.to_string(),
                block_timestamp: header.timestamp(),
            };

            tokio::spawn(async move {
                let _ = client.post_block_info(context::current(), body).await;
            });
        }
    }
}
