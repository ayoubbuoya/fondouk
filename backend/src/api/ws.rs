//! `GET /ws`: a one-way stream of live events (F8.5, HANDOFF D43).
//!
//! The server only talks; anything the browser sends, apart from closing,
//! is ignored. No login is needed: every event is public data (no persona),
//! so there is nothing a cross-site page could steal through this socket.

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use tokio::sync::broadcast::{Receiver, error::RecvError};

use super::AppState;
use crate::live_events::LiveEvent;

/// Upgrades the HTTP request to a WebSocket and starts streaming events.
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    // Subscribe before the upgrade finishes, so an event published in between
    // is not lost.
    let events = state.events.subscribe();
    ws.on_upgrade(move |socket| stream_events(socket, events))
}

/// Forwards every event as a JSON text message until the browser leaves.
///
/// A browser that falls more than the channel capacity behind loses the
/// oldest events (`Lagged`) instead of slowing the tick loop. We log it and
/// keep going; the frontend can refetch over REST.
async fn stream_events(mut socket: WebSocket, mut events: Receiver<LiveEvent>) {
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(event) => {
                    let text = match serde_json::to_string(&event) {
                        Ok(text) => text,
                        Err(err) => {
                            tracing::error!("could not serialize a live event: {err}");
                            continue;
                        }
                    };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break; // browser gone
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "a /ws client fell behind, events skipped");
                }
                Err(RecvError::Closed) => break, // backend shutting down
            },
            incoming = socket.recv() => match incoming {
                // Pings are answered by axum itself; other messages are ignored.
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{router, tests::test_state};
    use crate::live_events::publish;
    use futures_util::StreamExt;
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;
    use tokio_tungstenite::tungstenite::Message as ClientMessage;

    /// A real server on a random port (WebSockets need a real connection;
    /// the `oneshot` trick of the other tests cannot upgrade).
    #[tokio::test]
    async fn a_connected_client_receives_published_events() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://unused@localhost/unused")
            .unwrap();
        let state = test_state(pool);
        let events = state.events.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });

        let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
            .await
            .unwrap();
        // The handler subscribes during the upgrade; wait until it has.
        while events.receiver_count() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        publish(
            &events,
            LiveEvent::AvatarMove {
                avatar_id: uuid::Uuid::nil(),
                from: None,
                to: Some("rhea-lend-usdc".to_string()),
            },
        );

        let message = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .expect("no event within 5 s")
            .unwrap()
            .unwrap();
        let ClientMessage::Text(text) = message else {
            panic!("expected a text message, got {message:?}");
        };
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["type"], "avatar_move");
        assert_eq!(json["to"], "rhea-lend-usdc");
    }
}
