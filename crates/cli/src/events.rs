// subscription code is not in use yet
#![allow(dead_code)]

use std::{collections::HashSet, str::FromStr, time::Duration};

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use http::Uri;
use snops_common::events::{Event, EventFilter, EventWsRequest};
use tokio::{net::TcpStream, select};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{self, client::IntoClientRequest},
};

pub struct EventsClient {
    counter: u32,
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    subscriptions: HashSet<u32>,
    ping_interval: tokio::time::Interval,
}

impl EventsClient {
    pub async fn open(url: &str) -> Result<Self> {
        Self::new(url, None).await
    }

    pub async fn open_with_filter(url: &str, filter: EventFilter) -> Result<Self> {
        Self::new(url, Some(filter)).await
    }

    pub async fn new(url: &str, filter: Option<EventFilter>) -> Result<Self> {
        let (proto, hostname) = url.split_once("://").unwrap_or(("http", url));
        let proto = match proto {
            "wss" | "https" => "wss",
            _ => "ws",
        };

        let req = Uri::from_str(&match filter {
            Some(filter) => format!(
                "{proto}://{hostname}/api/v1/events?filter={}",
                urlencoding::encode(&filter.to_string())
            ),
            None => format!("{proto}://{hostname}/api/v1/events"),
        })
        .context("Invalid URI")?
        .into_client_request()
        .context("Invalid websocket request")?;

        let stream = match connect_async(req).await {
            Ok((stream, _)) => stream,
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                bail!("Failed to connect to websocket: Connection refused")
            }
            Err(e) => bail!("Failed to connect to websocket: {}", e),
        };

        Ok(Self {
            counter: 0,
            stream,
            subscriptions: Default::default(),
            ping_interval: tokio::time::interval(Duration::from_secs(10)),
        })
    }

    async fn send_json(&mut self, msg: impl serde::Serialize) -> Result<()> {
        self.stream
            .send(tungstenite::Message::Text(
                serde_json::to_string(&msg).context("Failed to serialize message")?,
            ))
            .await
            .context("Failed to send message")
    }

    /// Add an additional filter to the current subscription
    pub async fn subscribe(&mut self, filter: EventFilter) -> Result<u32> {
        let id = self.counter;
        self.send_json(EventWsRequest::Subscribe { id, filter })
            .await?;
        self.counter = self.counter.saturating_add(1);
        self.subscriptions.insert(id);
        Ok(id)
    }

    /// Remove a filter from the current subscription
    pub async fn unsubscribe(&mut self, id: u32) -> Result<()> {
        if !self.subscriptions.remove(&id) {
            bail!("Subscription not found: {}", id);
        }
        self.send_json(EventWsRequest::Unsubscribe { id }).await?;
        Ok(())
    }

    /// Remove all filters from the current subscription
    pub async fn unsubscribe_all(&mut self) -> Result<()> {
        // Collect the ids to avoid borrowing issues
        for id in self.subscriptions.drain().collect::<Vec<_>>() {
            self.send_json(EventWsRequest::Unsubscribe { id }).await?;
        }
        Ok(())
    }

    /// Get the next event from the stream
    pub async fn next(&mut self) -> Result<Option<Event>> {
        loop {
            select! {
                _ = tokio::signal::ctrl_c() => return Ok(None),
                _ = self.ping_interval.tick() => {
                    self.stream.send(tungstenite::Message::Ping(vec![b'p', b'i', b'n', b'g'])).await.context("Failed to send ping")?;
                }
                msg = self.stream.next() => {
                    match msg {
                        Some(Ok(tungstenite::Message::Text(text))) =>
                        return serde_json::from_str(&text).map(Some).with_context(|| format!("Failed to parse event: {text}")),
                        Some(Ok(tungstenite::Message::Binary(bin))) =>
                        return serde_json::from_slice(&bin).map(Some).with_context(|| format!("Failed to parse event: {}", String::from_utf8_lossy(&bin))),
                        None | Some(Err(_)) => bail!("Websocket closed"),
                        Some(Ok(_)) => continue,

                    }
                }
            }
        }
    }

    /// Close the websocket connection
    pub async fn close(mut self) -> Result<()> {
        self.stream.close(None).await?;
        Ok(())
    }
}
