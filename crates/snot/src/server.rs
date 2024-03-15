use std::time::Instant;

use anyhow::Result;
use axum::{response::IntoResponse, routing::get, Router};
use axum_typed_websockets::{Codec, Message, WebSocket, WebSocketUpgrade};
use serde::{de::DeserializeOwned, Serialize};
use snot_common::prelude::*;

type Socket = WebSocket<ServerMessage, ClientMessage, BinaryCodec>;
type SocketUpgrade = WebSocketUpgrade<ServerMessage, ClientMessage, BinaryCodec>;

pub async fn start() -> Result<()> {
    let app = Router::new().route("/agent", get(agent_ws_handler));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:1234").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn agent_ws_handler(ws: SocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: Socket) {
    let start = Instant::now();

    // send ping
    socket
        .send(Message::Item(ServerMessage::Ping))
        .await
        .unwrap();

    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Item(ClientMessage::Pong)) => {
                println!("elapsed: {:?}", start.elapsed())
            }
            Ok(_) => (),
            Err(err) => eprintln!("error: {err}"),
        }
    }

    println!("socket has closed");
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
