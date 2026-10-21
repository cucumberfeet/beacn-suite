//! Websocket workers for the Mixer page, run as iced subscriptions.
//!
//! `connection` holds the control socket: it fetches the status, forwards the daemon's live
//! patches and command replies, and sends whatever the page queues. It reconnects every second
//! while the daemon is down (for example while beacn.service is still starting it).
//!
//! `meters` holds the meter socket. It's only subscribed while the Mixer page is on screen,
//! because the daemon only runs its level meters while a meter client is connected.

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use log::{debug, warn};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;
use tokio_tungstenite_wasm::{Message, Utf8Bytes, connect};

const URL: &str = "ws://localhost:14565/api/websocket";
const METER_URL: &str = "ws://localhost:14565/api/websocket/meter";
const RETRY: Duration = Duration::from_secs(1);
const METER_BATCH: Duration = Duration::from_millis(33);

/// Command ids the page never uses, so the status reply can't be confused with anything else.
const STATUS_ID: u64 = u64::MAX - 1;

#[derive(Debug, Clone)]
pub struct Outgoing {
    pub id: u64,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// Connected; commands go through this sender until the next `Disconnected`
    Ready(mpsc::Sender<Outgoing>),
    /// The full `DaemonStatus`
    Status(Value),
    /// An RFC 6902 patch to apply to the status
    Patch(Value),
    /// The reply to one of the page's commands
    Response(u64, Value),
    Disconnected,
}

pub fn connection() -> impl Stream<Item = Event> {
    iced::stream::channel(128, async move |mut output: mpsc::Sender<Event>| {
        let mut was_connected = false;
        loop {
            match connect(URL).await {
                Ok(mut ws) => {
                    debug!("Mixer: connected to Pipeweaver");
                    let (tx, mut rx) = mpsc::channel::<Outgoing>(256);

                    let request = json!({ "id": STATUS_ID, "data": "GetStatus" }).to_string();
                    if ws
                        .send(Message::Text(Utf8Bytes::from(request)))
                        .await
                        .is_ok()
                    {
                        was_connected = true;
                        let _ = output.send(Event::Ready(tx)).await;

                        loop {
                            tokio::select! {
                                message = ws.next() => match message {
                                    Some(Ok(Message::Text(text))) => {
                                        let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
                                            warn!("Mixer: unreadable message from Pipeweaver");
                                            continue;
                                        };
                                        let id = value["id"].as_u64().unwrap_or(u64::MAX);
                                        let data = value.get("data").cloned().unwrap_or(Value::Null);
                                        let event = if let Some(patch) = data.get("Patch") {
                                            Event::Patch(patch.clone())
                                        } else if id == STATUS_ID && let Some(status) = data.get("Status") {
                                            Event::Status(status.clone())
                                        } else {
                                            Event::Response(id, data)
                                        };
                                        if output.send(event).await.is_err() {
                                            return;
                                        }
                                    }
                                    Some(Ok(Message::Close(_))) | None => break,
                                    Some(Ok(_)) => {}
                                    Some(Err(e)) => {
                                        debug!("Mixer: websocket error: {e}");
                                        break;
                                    }
                                },
                                outgoing = rx.next() => {
                                    let Some(outgoing) = outgoing else { break };
                                    let text = json!({ "id": outgoing.id, "data": outgoing.data }).to_string();
                                    if ws.send(Message::Text(Utf8Bytes::from(text))).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    let _ = ws.close().await;
                    if was_connected && output.send(Event::Disconnected).await.is_err() {
                        return;
                    }
                }
                Err(_) => {
                    if was_connected {
                        was_connected = false;
                        if output.send(Event::Disconnected).await.is_err() {
                            return;
                        }
                    }
                }
            }
            tokio::time::sleep(RETRY).await;
        }
    })
}

/// Meter levels (node id -> percent), batched to about 30 updates a second.
pub fn meters() -> impl Stream<Item = Vec<(String, u8)>> {
    iced::stream::channel(
        8,
        async move |mut output: mpsc::Sender<Vec<(String, u8)>>| {
            loop {
                if let Ok(mut ws) = connect(METER_URL).await {
                    let mut levels: HashMap<String, u8> = HashMap::new();
                    let mut tick = tokio::time::interval(METER_BATCH);
                    loop {
                        tokio::select! {
                            message = ws.next() => match message {
                                Some(Ok(Message::Text(text))) => {
                                    if let Ok(value) = serde_json::from_str::<Value>(text.as_str())
                                        && let (Some(id), Some(percent)) =
                                            (value["id"].as_str(), value["percent"].as_u64())
                                    {
                                        levels.insert(id.to_string(), percent.min(100) as u8);
                                    }
                                }
                                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                                Some(Ok(_)) => {}
                            },
                            _ = tick.tick() => {
                                if !levels.is_empty() {
                                    let batch: Vec<(String, u8)> = levels.drain().collect();
                                    if output.send(batch).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    let _ = ws.close().await;
                }
                tokio::time::sleep(RETRY).await;
            }
        },
    )
}
