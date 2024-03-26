use std::{sync::Arc, time::Duration};

use ::jwt::VerifyWithKey;
use anyhow::Result;
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::{HeaderMap, Request},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use futures_util::stream::StreamExt;
use snot_common::{
    prelude::*,
    rpc::{agent::AgentServiceClient, control::ControlService},
};
use surrealdb::Surreal;
use tarpc::server::Channel;
use tokio::select;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::{info, warn, Span};

use self::{
    jwt::{Claims, JWT_NONCE, JWT_SECRET},
    rpc::ControlRpcServer,
};
use crate::{
    cli::Cli,
    server::rpc::{MuxedMessageIncoming, MuxedMessageOutgoing},
    state::{Agent, AppState, GlobalState},
};

mod api;
mod content;
pub mod jwt;
mod rpc;

pub async fn start(cli: Cli) -> Result<()> {
    let mut path = cli.path.clone();
    path.push("data.db");
    let db = Surreal::new::<surrealdb::engine::local::File>(path).await?;
    let state = GlobalState {
        cli,
        db,
        pool: Default::default(),
        storage_ids: Default::default(),
        storage: Default::default(),
        tests_counter: Default::default(),
        tests: Default::default(),
    };

    let app =
        Router::new()
            .route("/agent", get(agent_ws_handler))
            .nest("/api/v1", api::routes())
            // /env/<id>/ledger/* - ledger query service reverse proxying /mainnet/latest/stateRoot
            .nest("/content", content::init_routes(&state).await)
            .with_state(Arc::new(state))
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(DefaultMakeSpan::new().include_headers(true)), /* .on_request(|request: &Request<Body>, _span: &Span| {
                                                                                    *     tracing::info!("req {} - {}", request.method(), request.uri());
                                                                                    * })
                                                                                    * .on_response(|response: &Response, _latency: Duration, span: &Span| {
                                                                                    *     span.record("status_code", &tracing::field::display(response.status()));
                                                                                    *     tracing::info!("res {}", response.status())
                                                                                    * }), */
            );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:1234").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn agent_ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, headers, state))
}

async fn handle_socket(mut socket: WebSocket, headers: HeaderMap, state: AppState) {
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
            // ensure the nonce is correct
            if claims.nonce == *JWT_NONCE {
                true
            } else {
                warn!("connecting agent specified invalid JWT nonce");
                false
            }
        });

    // TODO: the client should provide us with some information about itself (num
    // cpus, etc.) before we categorize it and add it as an agent to the agent pool

    // set up the RPC channels
    let (client_response_in, client_transport, mut client_request_out) = RpcTransport::new();
    let (server_request_in, server_transport, mut server_response_out) = RpcTransport::new();

    // set up the client, facing the agent server
    let client =
        AgentServiceClient::new(tarpc::client::Config::default(), client_transport).spawn();

    let id: usize = 'insertion: {
        let client = client.clone();
        let mut pool = state.pool.write().await;

        // attempt to reconnect if claims were passed
        'reconnect: {
            if let Some(claims) = claims {
                let Some(agent) = pool.get_mut(&claims.id) else {
                    warn!("connecting agent is trying to identify as an unrecognized agent");
                    break 'reconnect;
                };

                if agent.is_connected() {
                    warn!("connecting agent is trying to identify as an already-connected agent");
                    break 'reconnect;
                }

                agent.mark_connected(client);

                let id = agent.id();
                info!("agent {id} reconnected");

                // TODO: probably want to reconcile with old state?

                break 'insertion id;
            }
        }

        // otherwise, we need to create an agent and give it a new JWT

        // create a new agent
        let agent = Agent::new(client.to_owned());

        // sign the jwt and send it to the agent
        let signed_jwt = agent.sign_jwt();
        tokio::spawn(async move {
            // we do this in a separate task because we don't want to hold up pool insertion
            if let Err(e) = client.keep_jwt(tarpc::context::current(), signed_jwt).await {
                warn!("failed to inform client of JWT: {e}");
            }
        });

        // insert a new agent into the pool
        let id = agent.id();
        pool.insert(id, agent);

        info!("agent {id} connected; pool is now {} nodes", pool.len());

        id
    };

    // fetch the agent's network addresses on connect/reconnect
    let state2 = state.clone();
    tokio::spawn(async move {
        if let Ok((ports, external, internal)) = client.get_addrs(tarpc::context::current()).await {
            let mut state = state2.pool.write().await;
            if let Some(agent) = state.get_mut(&id) {
                info!("agent {id} addrs: {external:?} {internal:?} @ {ports}");
                agent.set_ports(ports);
                agent.set_addrs(external, internal);
            }
        }
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

    // remove the client from the agent in the agent pool
    {
        // TODO: remove agent after 10 minutes of inactivity

        let mut pool = state.pool.write().await;
        if let Some(agent) = pool.get_mut(&id) {
            agent.mark_disconnected();
        }

        info!("agent {id} disconnected");
    }
}
