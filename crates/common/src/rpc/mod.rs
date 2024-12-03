//! The module defining all RPC behaviors in snops.
//!
//! This module is split into two separate modules:
//! * `control`: the RPC server that lies on websockets established between the
//!   control plane and the agent, and
//! * `agent`: the RPC server that lies on websockets established between the
//!   agent and its AOT/snarkOS node.
//!
//! The naming convention for RPC-related modules is to name the modules after
//! the RPC's *parent*, where the parent is the side of the transport
//! responsible for *listening* for new websocket connections.

use std::{
    mem::size_of,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Sink, Stream};
use serde::{Deserialize, Serialize};
use tarpc::transport::channel::ChannelError;
use tokio::sync::mpsc;

pub mod agent;
pub mod codec;
pub mod control;
pub mod error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MuxMessage<Parent, Child> {
    Parent(Parent),
    Child(Child),
}

#[macro_export]
macro_rules! define_rpc_mux {
    ( parent ; $parent_req:ty => $parent_res:ty ; $child_req:ty => $child_res:ty $(;)? ) => {
        /// A multiplexed message, incoming on the websocket.
        pub type MuxedMessageIncoming = ::snops_common::rpc::MuxMessage<
            ::tarpc::ClientMessage<$parent_req>,
            ::tarpc::Response<$child_res>,
        >;

        /// A multiplexed message, outgoing on the websocket.
        pub type MuxedMessageOutgoing = ::snops_common::rpc::MuxMessage<
            ::tarpc::Response<$parent_res>,
            ::tarpc::ClientMessage<$child_req>,
        >;
    };
    ( child ; $parent_req:ty => $parent_res:ty ; $child_req:ty => $child_res:ty $(;)? ) => {
        /// A multiplexed message, incoming on the websocket.
        pub type MuxedMessageIncoming = ::snops_common::rpc::MuxMessage<
            ::tarpc::Response<$parent_res>,
            ::tarpc::ClientMessage<$child_req>,
        >;

        /// A multiplexed message, outgoing on the websocket.
        pub type MuxedMessageOutgoing = ::snops_common::rpc::MuxMessage<
            ::tarpc::ClientMessage<$parent_req>,
            ::tarpc::Response<$child_res>,
        >;
    };
}

pub const PING_LENGTH: usize = size_of::<u32>() + size_of::<u128>();
pub const PING_INTERVAL_SEC: u64 = 10;

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
