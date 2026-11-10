//! Live events pushed to every open browser over `/ws` (F8.5, HANDOFF D43).
//!
//! One `tokio::sync::broadcast` channel: the indexer and the agent tick loop
//! publish, and each WebSocket connection subscribes and gets its own copy,
//! like listeners of one radio station.
//!
//! ```text
//! indexer ──apr_update──┐
//!                       ├─> broadcast channel ─> /ws client 1, 2, ... N
//! tick ──decision──────┤
//!      ──avatar_move───┘
//! ```
//!
//! **Public data only.** Events never carry the persona (D7): it is not a
//! field of any variant, so it cannot be sent by mistake.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::api::villages::VillageDto;

/// How many events a slow client may fall behind before it starts losing the
/// oldest ones (`RecvError::Lagged`). A tick of 10 avatars sends at most 20
/// events, so 256 leaves a lot of room.
const CHANNEL_CAPACITY: usize = 256;

/// One message on `/ws`, serialized as JSON with a `type` field, e.g.
/// `{"type":"avatar_move","avatar_id":"…","from":null,"to":"rhea-lend-usdc"}`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveEvent {
    /// After each indexer round: the same list `GET /villages` returns.
    AprUpdate { villages: Vec<VillageDto> },
    /// After an avatar's tick is committed.
    Decision {
        avatar_id: Uuid,
        ts: DateTime<Utc>,
        /// The applied action (HOLD when the proposal was refused).
        action: Value,
        valid: bool,
        rejected_rule: Option<String>,
        reason_public: Option<String>,
        cost_usd: Decimal,
        equity_usd: Decimal,
    },
    /// The village holding most of the avatar's money changed, so its sprite
    /// walks there (TECH §8). `None` = all cash (no village).
    AvatarMove {
        avatar_id: Uuid,
        from: Option<String>,
        to: Option<String>,
    },
    /// A character posted a short message to the arena (SPEC F9.1).
    ArenaMessage {
        id: i64,
        avatar_id: Uuid,
        avatar_name: String,
        sprite: String,
        text: String,
        ts: DateTime<Utc>,
    },
}

/// The publishing side, cloned into the indexer, the tick loop and the API.
pub type LiveSender = broadcast::Sender<LiveEvent>;

/// Creates the channel. Receivers are created later with `subscribe()`.
pub fn channel() -> LiveSender {
    broadcast::channel(CHANNEL_CAPACITY).0
}

/// Sends an event to every connected client.
///
/// `send` fails only when nobody is listening (no browser open). That is
/// normal, not an error, so the result is ignored on purpose.
pub fn publish(sender: &LiveSender, event: LiveEvent) {
    let _ = sender.send(event);
}

/// The village where the avatar's sprite stands: the one holding the most
/// money. Ties go to the smallest village id (`BTreeMap` order), so the
/// answer is stable. `None` when the avatar holds only cash.
pub fn main_village(positions: &BTreeMap<String, Decimal>) -> Option<String> {
    let mut best: Option<(&String, &Decimal)> = None;
    for (village, value) in positions {
        // Strictly greater: on a tie the earlier (smaller) id stays.
        if best.is_none_or(|(_, best_value)| value > best_value) {
            best = Some((village, value));
        }
    }
    best.map(|(village, _)| village.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions(entries: &[(&str, i64)]) -> BTreeMap<String, Decimal> {
        entries
            .iter()
            .map(|(village, value)| (village.to_string(), Decimal::from(*value)))
            .collect()
    }

    #[test]
    fn main_village_is_the_largest_position() {
        let held = positions(&[("a", 10), ("b", 70), ("c", 20)]);
        assert_eq!(main_village(&held), Some("b".to_string()));
    }

    #[test]
    fn main_village_tie_goes_to_the_smallest_id() {
        let held = positions(&[("b", 50), ("a", 50)]);
        assert_eq!(main_village(&held), Some("a".to_string()));
    }

    #[test]
    fn main_village_of_an_all_cash_avatar_is_none() {
        assert_eq!(main_village(&BTreeMap::new()), None);
    }

    #[test]
    fn events_serialize_with_a_type_tag() {
        let event = LiveEvent::AvatarMove {
            avatar_id: Uuid::nil(),
            from: None,
            to: Some("rhea-lend-usdc".to_string()),
        };

        let json = serde_json::to_value(&event).unwrap();

        assert_eq!(json["type"], "avatar_move");
        assert!(json["from"].is_null());
        assert_eq!(json["to"], "rhea-lend-usdc");
    }

    #[test]
    fn publish_without_listeners_does_not_fail() {
        let sender = channel();
        publish(
            &sender,
            LiveEvent::AprUpdate {
                villages: Vec::new(),
            },
        );
    }

    #[tokio::test]
    async fn every_subscriber_gets_a_copy() {
        let sender = channel();
        let mut first = sender.subscribe();
        let mut second = sender.subscribe();
        let event = LiveEvent::AprUpdate {
            villages: Vec::new(),
        };

        publish(&sender, event.clone());

        assert_eq!(first.recv().await.unwrap(), event);
        assert_eq!(second.recv().await.unwrap(), event);
    }
}
