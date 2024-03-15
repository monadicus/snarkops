use futures_util::stream::StreamExt;
use snot_common::message::ServerMessage;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::main]
async fn main() {
    // TODO: clap args to specify where control plane is
    // TODO: TLS

    let (mut ws_stream, _) = connect_async("ws://127.0.0.1:1234/agent")
        .await
        .expect("connect to control plane");

    // TODO: real handler logic
    // TODO: ability to disconnect temporarily from the control plane (wait for it
    // to start)

    while let Ok(msg) = ws_stream.next().await.unwrap() {
        match msg {
            Message::Binary(payload) => {
                let message: ServerMessage = bincode::deserialize(&payload).expect("deserialize");

                match message {
                    ServerMessage::SetState(state) => {
                        println!("I have been instructed to set my state to {state:#?}");
                    }
                }
            }

            _ => (),
        }
    }
}
