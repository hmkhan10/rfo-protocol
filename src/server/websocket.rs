use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use dashmap::DashMap;
use futures::StreamExt;
use tokio::sync::broadcast;

use crate::protocol::WsMessage;
use crate::server::handlers::AppState;

// ── WebSocket Connection Manager ───────────────────────────────────────────

#[derive(Clone)]
pub struct WsManager {
    /// domain -> set of subscriber senders
    subscribers: Arc<DashMap<String, Vec<broadcast::Sender<String>>>>,
    /// Global broadcast for all updates
    global_tx: broadcast::Sender<String>,
}

impl WsManager {
    pub fn new() -> Self {
        let (global_tx, _) = broadcast::channel(256);
        Self {
            subscribers: Arc::new(DashMap::new()),
            global_tx,
        }
    }

    /// Publish a domain update to all subscribers.
    pub fn publish_update(&self, domain: &str, quality_score: u32) {
        let msg = serde_json::to_string(&WsMessage::Update {
            domain: domain.to_string(),
            quality_score,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
        .unwrap();

        // Send to domain-specific subscribers
        if let Some(tx) = self.subscribers.get(domain) {
            for sender in tx.iter() {
                let _ = sender.send(msg.clone());
            }
        }

        // Send to global subscribers
        let _ = self.global_tx.send(msg);
    }

    /// Subscribe to a specific domain.
    pub fn subscribe_domain(&self, domain: &str) -> broadcast::Receiver<String> {
        let (tx, rx) = broadcast::channel(64);
        self.subscribers
            .entry(domain.to_string())
            .or_default()
            .push(tx);
        rx
    }

    /// Subscribe to all updates.
    pub fn subscribe_global(&self) -> broadcast::Receiver<String> {
        self.global_tx.subscribe()
    }

    /// Get subscriber count for a domain.
    pub fn subscriber_count(&self, domain: &str) -> usize {
        self.subscribers
            .get(domain)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Total active connections.
    pub fn total_subscribers(&self) -> usize {
        self.subscribers.iter().map(|e| e.value().len()).sum()
    }
}

impl Default for WsManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── WebSocket Upgrade Handler ───────────────────────────────────────────────

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state.ws_manager))
}

// ── WebSocket Handler ──────────────────────────────────────────────────────

pub async fn handle_ws_connection(
    mut socket: WebSocket,
    manager: WsManager,
) {
    let mut subscriptions: Vec<String> = Vec::new();

    while let Some(result) = socket.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(_) => break,
        };

        match msg {
            Message::Text(text) => {
                let ws_msg: WsMessage = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(_) => {
                        let err = WsMessage::Error {
                            code: 400,
                            message: "Invalid message format".to_string(),
                        };
                        let _ = socket
                            .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                            .await;
                        continue;
                    }
                };

                match ws_msg {
                    WsMessage::Subscribe { domains } => {
                        for domain in &domains {
                            manager.subscribe_domain(domain);
                            subscriptions.push(domain.clone());
                        }
                        let _ = socket
                            .send(Message::Text(
                                serde_json::to_string(&serde_json::json!({
                                    "type": "subscribed",
                                    "domains": domains
                                }))
                                .unwrap()
                                .into(),
                            ))
                            .await;
                    }
                    WsMessage::Unsubscribe { domains } => {
                        for domain in &domains {
                            subscriptions.retain(|d| d != domain);
                        }
                        let _ = socket
                            .send(Message::Text(
                                serde_json::to_string(&serde_json::json!({
                                    "type": "unsubscribed",
                                    "domains": domains
                                }))
                                .unwrap()
                                .into(),
                            ))
                            .await;
                    }
                    WsMessage::Ping => {
                        let _ = socket
                            .send(Message::Text(
                                serde_json::to_string(&WsMessage::Pong).unwrap().into(),
                            ))
                            .await;
                    }
                    _ => {
                        let err = WsMessage::Error {
                            code: 400,
                            message: "Unsupported message type".to_string(),
                        };
                        let _ = socket
                            .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                            .await;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    tracing::info!(
        "WebSocket disconnected, {} subscriptions released",
        subscriptions.len()
    );
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_manager_publish() {
        let manager = WsManager::new();
        let mut rx = manager.subscribe_domain("example.com");

        manager.publish_update("example.com", 85);

        let msg = rx.try_recv().unwrap();
        let parsed: WsMessage = serde_json::from_str(&msg).unwrap();
        match parsed {
            WsMessage::Update {
                domain,
                quality_score,
                ..
            } => {
                assert_eq!(domain, "example.com");
                assert_eq!(quality_score, 85);
            }
            _ => panic!("Expected Update message"),
        }
    }

    #[test]
    fn test_ws_manager_global_subscribe() {
        let manager = WsManager::new();
        let mut rx = manager.subscribe_global();

        manager.publish_update("test.com", 50);

        let msg = rx.try_recv().unwrap();
        assert!(msg.contains("test.com"));
    }

    #[test]
    fn test_ws_manager_subscriber_count() {
        let manager = WsManager::new();
        assert_eq!(manager.subscriber_count("a.com"), 0);

        manager.subscribe_domain("a.com");
        manager.subscribe_domain("a.com");
        assert_eq!(manager.subscriber_count("a.com"), 2);

        assert_eq!(manager.total_subscribers(), 2);
    }
}
