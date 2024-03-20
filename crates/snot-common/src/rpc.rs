use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Sink, Stream};
use serde::{Deserialize, Serialize};
use tarpc::{client::RpcError, transport::channel::ChannelError};
use thiserror::Error;
use tokio::sync::mpsc;

use crate::state::AgentState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MuxMessage<Control, Agent> {
    Control(Control),
    Agent(Agent),
}

/// The RPC service that agents implement as a server.
#[tarpc::service]
pub trait AgentService {
    /// Control plane instructs the agent to use a JWT when reconnecting later.
    async fn keep_jwt(jwt: String);

    /// Control plane instructs the agent to reconcile towards a particular
    /// state.
    async fn reconcile(to: AgentState) -> Result<(), ()>; // TODO: return type
}

#[tarpc::service]
pub trait ControlService {
    async fn placeholder() -> String;
}

#[derive(Error, Debug)]
pub enum RpcErrorOr<O = ()> {
    #[error(transparent)]
    RpcError(#[from] RpcError),
    #[error("an error occurred")]
    Other(O),
}

pub struct RpcTransport<In, Out> {
    tx: mpsc::UnboundedSender<Out>,
    rx: mpsc::UnboundedReceiver<In>,
}

impl<In, Out> RpcTransport<In, Out> {
    /// Constructs a new RPC transport by building two channels. The returned
    /// transport can be used in as a tarpc transport, but the other tx and rx
    /// must be used to pass messages around as they come in/leave the
    /// websocket.
    pub fn new() -> (
        mpsc::UnboundedSender<In>,
        Self,
        mpsc::UnboundedReceiver<Out>,
    ) {
        let (tx1, rx1) = mpsc::unbounded_channel();
        let (tx2, rx2) = mpsc::unbounded_channel();
        (tx1, Self { tx: tx2, rx: rx1 }, rx2)
    }
}

impl<In, Out> Stream for RpcTransport<In, Out> {
    type Item = Result<In, ChannelError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx
            .poll_recv(cx)
            .map(|o| o.map(Ok))
            .map_err(ChannelError::Receive)
    }
}

const CLOSED_MESSAGE: &str = "the channel is closed";

impl<In, Out> Sink<Out> for RpcTransport<In, Out> {
    type Error = ChannelError;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(if self.tx.is_closed() {
            Err(ChannelError::Ready(CLOSED_MESSAGE.into()))
        } else {
            Ok(())
        })
    }

    fn start_send(self: Pin<&mut Self>, item: Out) -> Result<(), Self::Error> {
        self.tx
            .send(item)
            .map_err(|_| ChannelError::Send(CLOSED_MESSAGE.into()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
