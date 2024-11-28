use std::{sync::Arc, time::Duration};

use ::jwt::VerifyWithKey;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use futures_util::stream::StreamExt;
use http::StatusCode;
use serde::Deserialize;
use snops_common::{
    constant::HEADER_AGENT_KEY,
    prelude::*,
    rpc::control::{
        agent::{AgentServiceClient, Handshake},
        ControlService,
    },
};
use tarpc::server::Channel;
use tokio::select;
use tracing::{error, info, warn};

use super::{jwt::Claims, rpc::ControlRpcServer};
use crate::{
    server::{
        jwt::JWT_SECRET,
        rpc::{MuxedMessageIncoming, MuxedMessageOutgoing},
    },
    state::{Agent, AgentFlags, AppState},
};

#[derive(Debug, Deserialize)]
pub struct AgentWsQuery {
    pub id: Option<AgentId>,
    #[serde(flatten)]
    pub flags: AgentFlags,
}

pub async fn agent_ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<AgentWsQuery>,
) -> Response {
    match (&state.agent_key, headers.get(HEADER_AGENT_KEY)) {
        // assert key equals passed header
        (Some(key), Some(header)) if key == header.to_str().unwrap_or_default() => (),

        // forbid if key is incorrect
        (Some(_), _) => {
            warn!("An agent has attempted to connect with a mismatching agent key");
            return StatusCode::UNAUTHORIZED.into_response();
        }

        // allow if no key is present
        _ => (),
    }

    ws.on_upgrade(|socket| handle_socket(socket, headers, state, query))
        .into_response()
}

async fn handle_socket(
    mut socket: WebSocket,
    headers: HeaderMap,
    state: AppState,
    query: AgentWsQuery,
) {
    let claims = headers
        .get("Authorization")
        .and_then(|auth| -> Option<Claims> {
            let auth = auth.to_str().ok()?;
            if !auth.starts_with("Bearer ") {
                return None;
            }

            let token = &auth[7..];

            // get claims out of the specified JWT
            token.verify_with_key(&*JWT_SECRET).ok()
        })
        .filter(|claims| {
            // ensure the id is correct
            if let Some(id) = query.id {
                if claims.id != id {
                    warn!("connecting agent specified an id different than the claim");
                    return false;
                }
            }

            true
        });

    // TODO: the client should provide us with some information about itself (num
    // cpus, etc.) before we categorize it and add it as an agent to the agent pool

    // set up the RPC channels
    let (client_response_in, client_transport, mut client_request_out) = RpcTransport::new();
    let (server_request_in, server_transport, mut server_response_out) = RpcTransport::new();

    // set up the client, facing the agent server
    let client =
        AgentServiceClient::new(tarpc::client::Config::default(), client_transport).spawn();

    let id: AgentId = 'insertion: {
        let client = client.clone();
        let mut handshake = Handshake {
            loki: state.cli.loki.as_ref().map(|u| u.to_string()),
            ..Default::default()
        };

        // attempt to reconnect if claims were passed
        'reconnect: {
            if let Some(claims) = claims {
                let Some(mut agent) = state.pool.get_mut(&claims.id) else {
                    warn!("Connecting agent is trying to identify as an unrecognized agent");
                    break 'reconnect;
                };

                let id = agent.id();
                if agent.is_connected() {
                    warn!(
                        "Connecting agent is trying to identify as an already-connected agent {id}"
                    );
                    break 'reconnect;
                }

                // compare the stored nonce with the JWT's nonce
                if agent.claims().nonce != claims.nonce {
                    warn!("Connecting agent {id} is trying to identify with an invalid nonce");
                    break 'reconnect;
                }

                match agent.env() {
                    Some(env) if !state.envs.contains_key(&env) => {
                        info!("setting agent {id} to Inventory state due to missing env {env}");
                        agent.set_state(AgentState::Inventory);
                    }
                    _ => {}
                }

                // attach the current known agent state to the handshake
                agent.state().clone_into(&mut handshake.state);

                // mark the agent as connected, update the flags as well
                agent.mark_connected(client.clone(), query.flags);

                info!("Agent {id} reconnected");
                if let Err(e) = state.db.agents.save(&id, &agent) {
                    error!("failed to save agent {id} to the database: {e}");
                }

                // drop agent ref to allow for mutable borrow in handshake requests
                drop(agent);

                tokio::spawn(async move {
                    // we do this in a separate task because we don't want to hold up pool insertion
                    let mut ctx = tarpc::context::current();
                    ctx.deadline += Duration::from_secs(300);
                    match client.handshake(ctx, handshake).await {
                        Ok(()) => (),
                        Err(e) => error!("failed to perform agent {id} handshake: {e}"),
                    }
                });

                break 'insertion id;
            }
        }

        // otherwise, we need to create an agent and give it a new JWT
        // TODO: remove unnamed agents
        let id = query.id.unwrap_or_else(AgentId::rand);

        // check if an agent with this id is already online
        if state
            .pool
            .get(&id)
            .map(|a| a.is_connected())
            .unwrap_or_default()
        {
            warn!("An agent is trying to identify as an already-connected agent {id}");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }

        // create a new agent
        let agent = Agent::new(client.to_owned(), id, query.flags);

        // sign the jwt
        let signed_jwt = agent.sign_jwt();
        handshake.jwt = Some(signed_jwt);

        // handshake with the client
        tokio::spawn(async move {
            // we do this in a separate task because we don't want to hold up pool insertion
            let mut ctx = tarpc::context::current();
            ctx.deadline += Duration::from_secs(300);
            match client.handshake(ctx, handshake).await {
                Ok(()) => (),
                Err(e) => error!("failed to perform agent {id} handshake: {e}"),
            }
        });

        // insert a new agent into the pool
        if let Err(e) = state.db.agents.save(&id, &agent) {
            error!("failed to save agent {id} to the database: {e}");
        }
        state.pool.insert(id, agent);

        info!(
            "Agent {id} connected; pool is now {} nodes",
            state.pool.len()
        );

        id
    };

    // fetch the agent's network addresses on connect/reconnect
    let state2 = Arc::clone(&state);
    tokio::spawn(async move {
        let Ok((ports, external, internal)) = client.get_addrs(tarpc::context::current()).await
        else {
            return;
        };
        let Some(mut agent) = state2.pool.get_mut(&id) else {
            return;
        };

        info!(
            "Agent {id} [{}], labels: {:?}, addrs: {external:?} {internal:?} @ {ports}, local pk: {}",
            agent.modes(),
            agent.str_labels(),
            if agent.has_local_pk() { "yes" } else { "no" },
        );

        let is_port_change = agent.set_ports(ports);
        let is_ip_change = agent.set_addrs(external, internal);

        if let Err(e) = state2.db.agents.save(&id, &agent) {
            error!("failed to save agent {id} to the database: {e}");
        }

        if !is_ip_change && !is_port_change {
            return;
        }
        let Some(env_id) = agent.env() else {
            return;
        };
        drop(agent);
        let Some(env) = state2.get_env(env_id) else {
            return;
        };

        info!("Agent {id} updated its network addresses... Submitting changes to associated peers");
        env.update_peer_addr(&state2, id, is_port_change, is_ip_change)
            .await;
    });

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
                                error!("failed to deserialize a message from agent {id}: {e}");
                                break;
                            }
                        };

                        match msg {
                            MuxedMessageIncoming::Parent(msg) => {
                                if let Err(e) = server_request_in.send(msg) {
                                    error!("internal RPC channel closed: {e}");
                                    break;
                                }
                            },
                            MuxedMessageIncoming::Child(msg) => {
                                if let Err(e) = client_response_in.send(msg) {
                                    error!("internal RPC channel closed: {e}");
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
                let Some(msg) = msg else {
                    error!("Agent {id} internal RPC channel closed");
                    break;
                };
                let bin = match bincode::serialize(&MuxedMessageOutgoing::Child(msg)) {
                    Ok(bin) => bin,
                    Err(e) => {
                        error!("Agent {id} failed to serialize request: {e}");
                        break;
                    }
                };
                if let Err(e) = socket.send(Message::Binary(bin)).await {
                    error!("Agent {id} failed to send request to agent {id}: {e}");
                    break;
                }
            }

            // handle outgoing responses
            msg = server_response_out.recv() => {
                let Some(msg) = msg else {
                    error!("Agent {id} internal RPC channel closed");
                    break;
                };
                let bin = match bincode::serialize(&MuxedMessageOutgoing::Parent(msg)) {
                    Ok(bin) => bin,
                    Err(e) => {
                        error!("Agent {id} failed to serialize response: {e}");
                        break;
                    }
                };
                if let Err(e) = socket.send(Message::Binary(bin)).await {
                    error!("Agent {id} failed to send response to agent {id}: {e}");
                    break;
                }
            }
        }
    }

    // abort the RPC server handle
    server_handle.abort();

    // remove the client from the agent in the agent pool
    if let Some(mut agent) = state.pool.get_mut(&id) {
        agent.mark_disconnected();
    }

    info!("Agent {id} disconnected");
}
