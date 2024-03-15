use futures_util::{stream::StreamExt, SinkExt};
use snot_common::message::{ClientMessage, ServerMessage};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::main]
async fn main() {
    // TODO: clap args to specify where control plane is

    let (mut ws_stream, _) = connect_async("ws://127.0.0.1:1234/agent")
        .await
        .expect("connect to control plane");

    // TODO: real handler logic
    // TODO: ability to disconnect temporarily from the control plane (wait for it
    // to start)

    while let Ok(msg) = ws_stream.next().await.unwrap() {
        match msg {
            Message::Binary(payload) => {
                let de: ServerMessage = bincode::deserialize(&payload).expect("deserialize");

                if matches!(de, ServerMessage::Ping) {
                    println!("received ping");

                    ws_stream
                        .send(Message::Binary(
                            bincode::serialize(&ClientMessage::Pong).unwrap(),
                        ))
                        .await
                        .unwrap();
                }
            }

            _ => (),
        }
    }
}
