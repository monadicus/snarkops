use std::collections::HashMap;

use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use serde::Deserialize;
use snops_common::events::{EventFilter, EventWsRequest};
use tokio::select;

use crate::{events::EventSubscriber, state::AppState};

#[derive(Debug, Deserialize)]
pub struct EventWsQuery {
    #[serde(default)]
    pub filter: Option<EventFilter>,
}

pub async fn event_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<EventWsQuery>,
) -> Response {
    ws.on_upgrade(|socket| async {
        EventWsHandler::new(state, query.filter)
            .handle_ws(socket)
            .await
    })
}

struct EventWsHandler {
    base_filter: Option<EventFilter>,
    subscriber: EventSubscriber,
    extra_filters: HashMap<u32, EventFilter>,
}

impl EventWsHandler {
    fn new(state: AppState, base_filter: Option<EventFilter>) -> Self {
        let subscriber = match base_filter.clone() {
            Some(filter) => state.events.subscribe_on(filter),
            // Listen to no events by default
            None => state.events.subscribe_on(!EventFilter::Unfiltered),
        };
        Self {
            base_filter,
            subscriber,
            extra_filters: Default::default(),
        }
    }

    /// Update the subscriber filter based on the base filter and extra filters
    fn update_subscriber(&mut self) {
        if self.extra_filters.is_empty() && self.base_filter.is_none() {
            self.subscriber.set_filter(!EventFilter::Unfiltered);
            return;
        }

        let base_filter = self.base_filter.clone().unwrap_or(EventFilter::Unfiltered);

        self.subscriber.set_filter(
            base_filter
                & EventFilter::AnyOf(self.extra_filters.values().cloned().collect::<Vec<_>>()),
        );
    }

    /// Handle a request from the websocket to subscribe or unsubscribe from
    /// events
    fn handle_request(&mut self, req: EventWsRequest) {
        match req {
            EventWsRequest::Subscribe { id, filter } => {
                self.extra_filters.insert(id, filter);
                self.update_subscriber();
            }
            EventWsRequest::Unsubscribe { id } => {
                self.extra_filters.remove(&id);
                self.update_subscriber();
            }
        }
    }

    /// Handle the websocket connection, sending events to the client and
    /// handling requests to subscribe or unsubscribe from the client
    async fn handle_ws(&mut self, mut socket: WebSocket) {
        loop {
            select! {
                msg = socket.recv() => {
                    // Parse the message
                    let req = match msg {
                        Some(Ok(Message::Text(text))) => serde_json::from_str::<EventWsRequest>(&text),
                        Some(Ok(Message::Binary(bin))) => serde_json::from_slice::<EventWsRequest>(&bin),
                        Some(Err(_)) | None => break,
                        _ => continue,
                    };
                    // Handle the request
                    match req {
                        Ok(req) => self.handle_request(req),
                        Err(_e) => break,
                    }
                }
                // Forward events to the client
                Ok(event) = self.subscriber.next() => {
                    let json = match serde_json::to_string(&event) {
                        Ok(json) => json,
                        Err(e) => {
                            tracing::error!("failed to serialize event for websocket: {e}");
                            break;
                        }
                    };
                    if let Err(e) = socket.send(Message::Text(json)).await {
                        tracing::error!("failed to send event to websocket: {e}");
                        break;
                    }
                }
            }
        }
    }
}
