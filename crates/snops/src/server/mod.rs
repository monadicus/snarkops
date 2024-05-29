use std::{sync::Arc, time::Duration};

use ::jwt::VerifyWithKey;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::HeaderMap,
    middleware,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use futures_util::stream::StreamExt;
use http::StatusCode;
use prometheus_http_query::Client as PrometheusClient;
use serde::Deserialize;
use snops_common::{
    constant::HEADER_AGENT_KEY,
    prelude::*,
    rpc::{
        agent::{AgentServiceClient, Handshake},
        control::ControlService,
    },
};
use tarpc::server::Channel;
use tokio::select;
use tracing::{error, info, warn};

use self::{
    error::StartError,
    jwt::{Claims, JWT_SECRET},
    rpc::ControlRpcServer,
};
use crate::{
    cli::Cli,
    db,
    logging::{log_request, req_stamp},
    server::rpc::{MuxedMessageIncoming, MuxedMessageOutgoing},
    state::{Agent, AgentFlags, AppState, GlobalState},
};

mod actions;
mod api;
mod content;
pub mod error;
pub mod jwt;
pub mod models;
pub mod prometheus;
mod rpc;

pub async fn start(cli: Cli) -> Result<(), StartError> {
    let db = db::Database::open(&cli.path.join("store"))?;

    let prometheus = cli
        .prometheus
        .as_ref()
        .and_then(|p| PrometheusClient::try_from(p.as_str()).ok());

    let state = GlobalState::load(cli, db, prometheus).await?;

    let app = Router::new()
        .route("/agent", get(agent_ws_handler))
        .nest("/api/v1", api::routes())
        .nest("/prometheus", prometheus::routes())
        .nest("/content", content::init_routes(&state).await)
        .with_state(state)
        .layer(middleware::map_response(log_request))
        .layer(middleware::from_fn(req_stamp));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:1234")
        .await
        .map_err(StartError::TcpBind)?;
    axum::serve(listener, app)
        .await
        .map_err(StartError::Serve)?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct AgentWsQuery {
    id: Option<AgentId>,
    #[serde(flatten)]
    flags: AgentFlags,
}

async fn agent_ws_handler(
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
            warn!("an agent has attempted to connect with a mismatching agent key");
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
                    warn!("connecting agent is trying to identify as an unrecognized agent");
                    break 'reconnect;
                };

                let id = agent.id();
                if agent.is_connected() {
                    warn!(
                        "connecting agent is trying to identify as an already-connected agent {id}"
                    );
                    break 'reconnect;
                }

                // compare the stored nonce with the JWT's nonce
                if agent.claims().nonce != claims.nonce {
                    warn!("connecting agent {id} is trying to identify with an invalid nonce");
                    break 'reconnect;
                }

                if let AgentState::Node(env, _) = agent.state() {
                    if !state.envs.contains_key(env) {
                        info!("setting agent {id} to Inventory state due to missing env {env}");
                        agent.set_state(AgentState::Inventory);
                    }
                }

                // attach the current known agent state to the handshake
                agent.state().clone_into(&mut handshake.state);

                // mark the agent as connected, update the flags as well
                agent.mark_connected(client, query.flags);

                info!("agent {id} reconnected");
                if let Err(e) = state.db.agents.save(&id, &agent) {
                    error!("failed to save agent {id} to the database: {e}");
                }

                // handshake with client
                // note: this may cause a reconciliation, so this *may* be non-instant
                // unwrap safety: this agent was just `mark_connected` with a valid client
                let client = agent.rpc().cloned().unwrap();
                tokio::spawn(async move {
                    // we do this in a separate task because we don't want to hold up pool insertion
                    let mut ctx = tarpc::context::current();
                    ctx.deadline += Duration::from_secs(300);
                    match client.handshake(ctx, handshake).await {
                        Ok(Ok(())) => (),
                        Ok(Err(e)) => {
                            error!("failed to perform agent {id} handshake reconciliation: {e}")
                        }
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
            warn!("an agent is trying to identify as an already-connected agent {id}");
            socket.send(Message::Close(None)).await.ok();
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
                Ok(Ok(())) => (),
                Ok(Err(e)) => error!("failed to perform agent {id} handshake reconciliation: {e}"),
                Err(e) => error!("failed to perform agent {id} handshake: {e}"),
            }
        });

        // insert a new agent into the pool
        if let Err(e) = state.db.agents.save(&id, &agent) {
            error!("failed to save agent {id} to the database: {e}");
        }
        state.pool.insert(id, agent);

        info!(
            "agent {id} connected; pool is now {} nodes",
            state.pool.len()
        );

        id
    };

    // fetch the agent's network addresses on connect/reconnect
    let state2 = Arc::clone(&state);
    tokio::spawn(async move {
        if let Ok((ports, external, internal)) = client.get_addrs(tarpc::context::current()).await {
            if let Some(mut agent) = state2.pool.get_mut(&id) {
                info!(
                    "agent {id} [{}], labels: {:?}, addrs: {external:?} {internal:?} @ {ports}, local pk: {}",
                    agent.modes(),
                    agent.str_labels(),
                    if agent.has_local_pk() { "yes" } else { "no" },
                );
                agent.set_ports(ports);
                agent.set_addrs(external, internal);
                if let Err(e) = state2.db.agents.save(&id, &agent) {
                    error!("failed to save agent {id} to the database: {e}");
                }
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
                                error!("failed to deserialize a message from agent {id}: {e}");
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
                if (socket.send(Message::Binary(bin)).await).is_err() {
                    break;
                }
            }

            // handle outgoing responses
            msg = server_response_out.recv() => {
                let msg = msg.expect("internal RPC channel closed");
                let bin = bincode::serialize(&MuxedMessageOutgoing::Control(msg)).expect("failed to serialize response");
                if (socket.send(Message::Binary(bin)).await).is_err() {
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

        if let Some(mut agent) = state.pool.get_mut(&id) {
            agent.mark_disconnected();
        }

        info!("agent {id} disconnected");
    }
}
