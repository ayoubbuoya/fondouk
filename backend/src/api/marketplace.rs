//! Marketplace endpoints (F11).
//!
//! | Route | Who | Notes |
//! | --- | --- | --- |
//! | `GET /marketplace/listings` | anyone | active listings read from the contract, joined with public avatar data |
//! | `POST /marketplace/claim` | logged in | turns a paid `buy_clone` transaction into a clone |
//!
//! **How a clone is created (Day 9, option C).** The buyer pays in their
//! wallet, then the browser sends us the transaction hash. We do not trust
//! the browser: we read the transaction from the NEAR RPC and look for the
//! `clone_purchased` event that **our marketplace contract** emitted. Only
//! then do we create the clone. The hash is the primary key of
//! `processed_purchases`, so claiming twice never creates two clones.
//!
//! ```text
//! browser ──buy_clone + NEAR──▶ marketplace contract ──event──▶ chain
//! browser ──POST /marketplace/claim {tx_hash}──▶ backend
//!                         backend ──tx status──▶ NEAR RPC
//!                         backend: event from our contract? buyer == logged in?
//!                         backend: INSERT clone + processed_purchases (one DB tx)
//! ```
//!
//! **Persona privacy (D7).** The clone points to the source's strategy row
//! (`config_id`). The persona text is never copied or returned here.

use std::str::FromStr;

use axum::{Json, extract::State, http::StatusCode};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::auth::AuthUser;
use super::near_rpc;
use super::{ApiError, AppState};
use crate::db::avatars::{self, NewClone};
use crate::db::purchases::{self, NewPurchase};

/// A clone starts with this much paper money, like a new avatar (approved 6 Nov).
const CLONE_STARTING_CAPITAL_USD: u64 = 10_000;
/// Longest name the database accepts (`avatars.name` CHECK).
const MAX_NAME_CHARS: usize = 32;
const CLONE_SUFFIX: &str = " (clone)";
/// How many listings one request asks the contract for.
const LISTINGS_PAGE_SIZE: u32 = 50;
/// The NEP-297 `standard` and `event` our contract emits.
const EVENT_STANDARD: &str = "fondouk_marketplace";
const EVENT_NAME: &str = "clone_purchased";
const EVENT_PREFIX: &str = "EVENT_JSON:";

/// One character on sale. Public data only (no persona, D7).
#[derive(Debug, Serialize)]
pub struct ListingDto {
    pub avatar_id: Uuid,
    pub name: String,
    pub sprite: String,
    pub risk: String,
    pub goal: String,
    /// The account that gets 90% of the price.
    pub creator: String,
    /// Price in yoctoNEAR, as a string (it does not fit a JSON number).
    pub price_yocto: String,
}

/// Body of `POST /marketplace/claim`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRequest {
    tx_hash: String,
}

/// Answer of `POST /marketplace/claim`.
#[derive(Debug, Serialize)]
pub struct ClaimDto {
    /// The buyer's new character.
    pub avatar_id: Uuid,
    /// `true` when this transaction was claimed before (nothing new created).
    pub already_claimed: bool,
}

/// What the contract's `clone_purchased` event says.
#[derive(Debug, PartialEq, Eq)]
struct Purchase {
    avatar_id: Uuid,
    buyer: String,
    creator: String,
    price_yocto: Decimal,
}

/// A listing as the contract returns it (`get_listings`). Extra fields are ignored.
#[derive(Debug, Deserialize)]
struct ChainListing {
    creator: String,
    price: String,
}

/// `GET /marketplace/listings`: what is for sale right now.
///
/// The contract is the source of truth for price and creator. A listing whose
/// avatar is unknown to our database, or is itself a clone, is skipped.
/// One query per listing (at most [`LISTINGS_PAGE_SIZE`]): fine at this scale.
pub async fn list_listings(
    State(state): State<AppState>,
) -> Result<Json<Vec<ListingDto>>, ApiError> {
    let answer = near_rpc::view_function(
        &state.http,
        &state.near_rpc_url,
        &state.marketplace_contract_id,
        "get_listings",
        &json!({ "from": 0, "limit": LISTINGS_PAGE_SIZE }),
    )
    .await?;
    let on_chain: Vec<(String, ChainListing)> = serde_json::from_value(answer)?;

    let mut listings = Vec::new();
    for (avatar_id, listing) in on_chain {
        let Ok(avatar_id) = Uuid::parse_str(&avatar_id) else {
            continue;
        };
        let Some(avatar) = avatars::get_avatar_with_config(&state.db, avatar_id).await? else {
            continue;
        };
        if avatar.cloned_from.is_some() {
            continue; // F11.5: clones are never sold
        }
        listings.push(ListingDto {
            avatar_id,
            name: avatar.name,
            sprite: avatar.sprite,
            risk: avatar.risk,
            goal: avatar.goal,
            creator: listing.creator,
            price_yocto: listing.price,
        });
    }
    Ok(Json(listings))
}

/// `POST /marketplace/claim {tx_hash}`: creates the clone for a purchase.
///
/// 201 with the new clone, or 200 with `already_claimed` when this hash was
/// claimed before. 400 when the transaction is unknown, not final, failed or
/// holds no purchase of ours; 403 when the buyer in the event is not the
/// logged-in account (nobody can claim someone else's purchase).
pub async fn claim_purchase(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<ClaimRequest>,
) -> Result<(StatusCode, Json<ClaimDto>), ApiError> {
    let tx_hash = request.tx_hash.trim();
    if !is_valid_tx_hash(tx_hash) {
        return Err(ApiError::BadRequest("tx_hash is not valid".to_string()));
    }

    // Already done? Answer without calling the RPC.
    if let Some(avatar_id) = purchases::find_clone_for_tx(&state.db, tx_hash).await? {
        return Ok(already_claimed(avatar_id));
    }

    let transaction = near_rpc::fetch_transaction(
        &state.http,
        &state.near_rpc_url,
        tx_hash,
        &user.account_id,
    )
    .await?
    .ok_or_else(|| ApiError::BadRequest("transaction not found".to_string()))?;
    let purchase = parse_purchase(&transaction, &state.marketplace_contract_id)
        .map_err(ApiError::BadRequest)?;
    if purchase.buyer != user.account_id {
        return Err(ApiError::Forbidden);
    }

    let (avatar_id, created) = create_clone(&state, tx_hash, &purchase).await?;
    if !created {
        return Ok(already_claimed(avatar_id));
    }
    tracing::info!(clone = %avatar_id, buyer = %user.account_id, tx = tx_hash, "clone created");
    Ok((
        StatusCode::CREATED,
        Json(ClaimDto {
            avatar_id,
            already_claimed: false,
        }),
    ))
}

fn already_claimed(avatar_id: Uuid) -> (StatusCode, Json<ClaimDto>) {
    (
        StatusCode::OK,
        Json(ClaimDto {
            avatar_id,
            already_claimed: true,
        }),
    )
}

/// Inserts the clone and the purchase record in one database transaction.
/// Returns the clone id and `true` when created, or the existing clone id and
/// `false` when another request claimed the same hash first.
async fn create_clone(
    state: &AppState,
    tx_hash: &str,
    purchase: &Purchase,
) -> Result<(Uuid, bool), ApiError> {
    let source = avatars::get_avatar_with_config(&state.db, purchase.avatar_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("the purchased character does not exist".to_string()))?;
    if source.cloned_from.is_some() {
        return Err(ApiError::BadRequest("a clone cannot be bought".to_string()));
    }

    let mut tx = state.db.begin().await?;
    let clone_id = avatars::insert_clone(
        &mut *tx,
        &NewClone {
            owner: purchase.buyer.clone(),
            source_id: source.id,
            config_id: source.config_id,
            name: clone_name(&source.name),
            sprite: source.sprite.clone(),
            starting_capital_usd: Decimal::from(CLONE_STARTING_CAPITAL_USD),
        },
    )
    .await?;
    let inserted = purchases::insert_purchase(
        &mut *tx,
        &NewPurchase {
            tx_hash: tx_hash.to_string(),
            source_avatar_id: source.id,
            clone_avatar_id: clone_id,
            buyer: purchase.buyer.clone(),
            creator: purchase.creator.clone(),
            price_yocto: purchase.price_yocto,
        },
    )
    .await?;
    if inserted {
        tx.commit().await?;
        return Ok((clone_id, true));
    }

    // Lost a race: the same hash was claimed a moment ago. Drop our clone.
    tx.rollback().await?;
    let existing = purchases::find_clone_for_tx(&state.db, tx_hash)
        .await?
        .ok_or_else(|| anyhow::anyhow!("purchase {tx_hash} vanished after a conflict"))?;
    Ok((existing, false))
}

/// "<source name> (clone)", cut so the whole name fits the 32-character limit.
fn clone_name(source_name: &str) -> String {
    let room = MAX_NAME_CHARS - CLONE_SUFFIX.chars().count();
    let base: String = source_name.chars().take(room).collect();
    format!("{base}{CLONE_SUFFIX}")
}

/// A NEAR transaction hash is 32 bytes in base58: 43 or 44 characters. A loose
/// shape check (letters and digits, sane length) before we call the RPC.
fn is_valid_tx_hash(hash: &str) -> bool {
    (40..=50).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Finds the one `clone_purchased` event of `contract_id` in an RPC `tx`
/// answer. Pure (JSON in, result out), so every rule is unit-tested.
///
/// Accepted only when: the transaction is final and did not fail, and the
/// event was logged by a successful receipt executed **by the marketplace
/// contract** (another contract could print the same text). A transaction
/// with no such event, or with several, is refused.
fn parse_purchase(transaction: &Value, contract_id: &str) -> Result<Purchase, String> {
    if transaction["final_execution_status"] != "FINAL" {
        return Err("transaction is not final yet, try again in a few seconds".to_string());
    }
    if transaction["status"].get("Failure").is_some() {
        return Err("transaction failed".to_string());
    }

    let mut events = Vec::new();
    let outcomes = transaction["receipts_outcome"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    for receipt in outcomes {
        let outcome = &receipt["outcome"];
        if outcome["executor_id"] != contract_id || outcome["status"].get("Failure").is_some() {
            continue;
        }
        for log in outcome["logs"].as_array().into_iter().flatten() {
            if let Some(event) = log.as_str().and_then(parse_event_log) {
                events.push(event);
            }
        }
    }
    match events.len() {
        1 => events.remove(0),
        0 => Err("no clone purchase found in this transaction".to_string()),
        _ => Err("this transaction holds several purchases, which is not supported".to_string()),
    }
}

/// Reads one log line. `None` for any log that is not our event; `Some(Err)`
/// for our event with unusable data.
fn parse_event_log(log: &str) -> Option<Result<Purchase, String>> {
    let event: Value = serde_json::from_str(log.strip_prefix(EVENT_PREFIX)?).ok()?;
    if event["standard"] != EVENT_STANDARD || event["event"] != EVENT_NAME {
        return None;
    }
    // NEP-297 allows `data` to be an object or a one-item list.
    let data = match &event["data"] {
        Value::Array(items) => items.first().unwrap_or(&Value::Null),
        other => other,
    };
    Some(purchase_from_data(data))
}

fn purchase_from_data(data: &Value) -> Result<Purchase, String> {
    let text = |key: &str| {
        data[key]
            .as_str()
            .ok_or_else(|| format!("purchase event has no {key}"))
    };
    let avatar_id = Uuid::parse_str(text("avatar_id")?)
        .map_err(|_| "purchase event has a bad avatar_id".to_string())?;
    let price = text("price")?;
    // Digits only: `Decimal::from_str` would also accept "1.5" or "-3".
    if price.is_empty() || !price.bytes().all(|b| b.is_ascii_digit()) {
        return Err("purchase event has a bad price".to_string());
    }
    // `Decimal` holds up to ~79,000 NEAR; a larger price is refused, not rounded.
    let price_yocto = Decimal::from_str(price)
        .map_err(|_| "purchase price is too large to store".to_string())?;
    if price_yocto.is_zero() {
        return Err("purchase event has a zero price".to_string());
    }
    Ok(Purchase {
        avatar_id,
        buyer: text("buyer")?.to_string(),
        creator: text("creator")?.to_string(),
        price_yocto,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::auth::tests::{login_cookie, state_with_rpc};
    use crate::api::near_rpc::tests::{fake_rpc, full_access_answer};
    use crate::api::router;
    use crate::api::tests::{json as body_json, request, send};
    use crate::db::avatars::{NewAvatar, NewConfig};
    use sqlx::PgPool;

    const CONTRACT: &str = "marketplace.test"; // same as `test_state`
    const TX_HASH: &str = "8dZPc7uSBn6ugsELuiB5wNrYyi3gPdbMXHNWBZNQGEDM";
    const OTHER_TX_HASH: &str = "9dZPc7uSBn6ugsELuiB5wNrYyi3gPdbMXHNWBZNQGEDM";
    const PERSONA: &str = "Secret plan: always chase the highest lending rate on Tuesdays.";
    const PRICE: &str = "100000000000000000000000"; // 0.1 NEAR

    fn event_log(avatar_id: Uuid, buyer: &str) -> String {
        format!(
            "{EVENT_PREFIX}{}",
            json!({
                "standard": EVENT_STANDARD, "version": "1.0.0", "event": EVENT_NAME,
                "data": { "avatar_id": avatar_id, "buyer": buyer,
                          "creator": "creator.testnet", "price": PRICE }
            })
        )
    }

    /// The `result` part of an RPC `tx` answer for a successful purchase.
    fn tx_result(avatar_id: Uuid, buyer: &str) -> Value {
        json!({
            "final_execution_status": "FINAL",
            "status": { "SuccessValue": "" },
            "transaction": { "signer_id": buyer, "receiver_id": CONTRACT },
            "transaction_outcome": { "id": TX_HASH, "outcome": {
                "logs": [], "executor_id": buyer, "status": { "SuccessReceiptId": "r1" } } },
            "receipts_outcome": [
                { "id": "r1", "outcome": { "logs": [event_log(avatar_id, buyer)],
                    "executor_id": CONTRACT, "status": { "SuccessValue": "" } } },
                { "id": "r2", "outcome": { "logs": [],
                    "executor_id": "creator.testnet", "status": { "SuccessValue": "" } } }
            ]
        })
    }

    fn rpc_answer(result: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": "fondouk", "result": result })
    }

    // ---- pure parsing ----

    #[test]
    fn a_valid_purchase_is_read_from_the_contract_event() {
        let id = Uuid::from_u128(0xABCD);
        let purchase = parse_purchase(&tx_result(id, "bob.testnet"), CONTRACT).unwrap();

        assert_eq!(purchase.avatar_id, id);
        assert_eq!(purchase.buyer, "bob.testnet");
        assert_eq!(purchase.creator, "creator.testnet");
        assert_eq!(purchase.price_yocto, Decimal::from_str(PRICE).unwrap());
    }

    #[test]
    fn event_data_as_a_one_item_list_is_accepted() {
        let id = Uuid::from_u128(0xABCD);
        let mut tx = tx_result(id, "bob.testnet");
        let log = format!(
            "{EVENT_PREFIX}{}",
            json!({ "standard": EVENT_STANDARD, "version": "1.0.0", "event": EVENT_NAME,
                "data": [{ "avatar_id": id, "buyer": "bob.testnet",
                           "creator": "creator.testnet", "price": PRICE }] })
        );
        tx["receipts_outcome"][0]["outcome"]["logs"] = json!([log]);

        assert!(parse_purchase(&tx, CONTRACT).is_ok());
    }

    #[test]
    fn a_transaction_that_is_not_final_or_failed_is_refused() {
        let id = Uuid::from_u128(0xABCD);
        let mut pending = tx_result(id, "bob.testnet");
        pending["final_execution_status"] = json!("EXECUTED_OPTIMISTIC");
        let mut failed = tx_result(id, "bob.testnet");
        failed["status"] = json!({ "Failure": { "ActionError": {} } });

        assert!(parse_purchase(&pending, CONTRACT).unwrap_err().contains("final"));
        assert!(parse_purchase(&failed, CONTRACT).unwrap_err().contains("failed"));
    }

    #[test]
    fn an_event_logged_by_another_contract_is_ignored() {
        let id = Uuid::from_u128(0xABCD);
        let mut tx = tx_result(id, "bob.testnet");
        tx["receipts_outcome"][0]["outcome"]["executor_id"] = json!("evil.testnet");

        let err = parse_purchase(&tx, CONTRACT).unwrap_err();
        assert!(err.contains("no clone purchase"), "{err}");
    }

    #[test]
    fn an_event_from_a_failed_receipt_is_ignored() {
        let id = Uuid::from_u128(0xABCD);
        let mut tx = tx_result(id, "bob.testnet");
        tx["receipts_outcome"][0]["outcome"]["status"] = json!({ "Failure": {} });

        assert!(parse_purchase(&tx, CONTRACT).is_err());
    }

    #[test]
    fn other_logs_and_other_events_are_ignored() {
        let id = Uuid::from_u128(0xABCD);
        let mut tx = tx_result(id, "bob.testnet");
        let other = format!(
            "{EVENT_PREFIX}{}",
            json!({ "standard": "nep171", "version": "1.0.0", "event": "nft_mint", "data": [] })
        );
        tx["receipts_outcome"][0]["outcome"]["logs"] =
            json!(["hello", other, event_log(id, "bob.testnet")]);

        assert!(parse_purchase(&tx, CONTRACT).is_ok());
    }

    #[test]
    fn two_purchases_in_one_transaction_are_refused() {
        let id = Uuid::from_u128(0xABCD);
        let mut tx = tx_result(id, "bob.testnet");
        tx["receipts_outcome"][0]["outcome"]["logs"] =
            json!([event_log(id, "bob.testnet"), event_log(id, "bob.testnet")]);

        assert!(parse_purchase(&tx, CONTRACT).unwrap_err().contains("several"));
    }

    #[test]
    fn a_bad_event_payload_is_refused() {
        let id = Uuid::from_u128(0xABCD);
        for (key, bad) in [
            ("avatar_id", json!("not-a-uuid")),
            ("price", json!("1.5")),
            ("price", json!("-3")),
            ("price", json!("0")),
            ("price", json!("")),
            ("price", json!("9".repeat(40))),
            ("buyer", json!(7)),
        ] {
            let mut data = json!({ "avatar_id": id, "buyer": "bob.testnet",
                                   "creator": "creator.testnet", "price": PRICE });
            data[key] = bad.clone();
            assert!(purchase_from_data(&data).is_err(), "{key} = {bad}");
        }
    }

    #[test]
    fn clone_names_fit_the_database_limit() {
        assert_eq!(clone_name("Hanna"), "Hanna (clone)");
        let long = clone_name(&"é".repeat(32));
        assert_eq!(long.chars().count(), 32);
        assert!(long.ends_with(CLONE_SUFFIX));
    }

    #[test]
    fn tx_hash_shape_is_checked() {
        assert!(is_valid_tx_hash(TX_HASH));
        assert!(!is_valid_tx_hash(""));
        assert!(!is_valid_tx_hash("short"));
        assert!(!is_valid_tx_hash(&format!("{TX_HASH}/../x")));
    }

    // ---- endpoints ----

    /// The creator's avatar with a private persona. Returns (avatar id, config id).
    async fn seed_source(pool: &PgPool) -> (Uuid, Uuid) {
        avatars::ensure_user(pool, "creator.testnet").await.unwrap();
        let config_id = avatars::insert_config(
            pool,
            &NewConfig {
                persona: PERSONA.to_string(),
                risk: "high".to_string(),
                goal: "max_yield".to_string(),
                created_by: "creator.testnet".to_string(),
            },
        )
        .await
        .unwrap();
        let id = avatars::insert_avatar(
            pool,
            &NewAvatar {
                owner: "creator.testnet".to_string(),
                config_id,
                name: "Hanna".to_string(),
                sprite: "boy".to_string(),
                mode: "sim".to_string(),
                starting_capital_usd: Decimal::from(10_000),
            },
        )
        .await
        .unwrap();
        (id, config_id)
    }

    /// A state whose fake RPC answers `result` to every call, and a session
    /// cookie for `account` (login uses the pool, not the RPC of this state).
    async fn state_and_cookie(pool: PgPool, account: &str, result: Value) -> (AppState, String) {
        let login_state = state_with_rpc(pool, full_access_answer()).await;
        let cookie = login_cookie(&login_state, account).await;
        let state = AppState {
            near_rpc_url: fake_rpc(rpc_answer(result)).await,
            ..login_state
        };
        (state, cookie)
    }

    async fn claim(
        state: &AppState,
        cookie: Option<&str>,
        tx_hash: &str,
    ) -> (StatusCode, Value) {
        let (status, _, body) = send(
            router(state.clone()),
            request(
                "POST",
                "/marketplace/claim",
                cookie,
                Some(json!({ "tx_hash": tx_hash })),
            ),
        )
        .await;
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    async fn avatar_count(pool: &PgPool) -> i64 {
        sqlx::query_scalar!("SELECT count(*) FROM avatars")
            .fetch_one(pool)
            .await
            .unwrap()
            .unwrap()
    }

    #[sqlx::test]
    async fn a_valid_claim_creates_a_clone_that_shares_the_strategy(pool: PgPool) {
        let (source, config_id) = seed_source(&pool).await;
        let (state, bob) =
            state_and_cookie(pool.clone(), "bob.testnet", tx_result(source, "bob.testnet")).await;

        let (status, body) = claim(&state, Some(&bob), TX_HASH).await;

        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["already_claimed"], false);
        assert!(!body.to_string().contains("Secret plan"));
        let clone_id: Uuid = body["avatar_id"].as_str().unwrap().parse().unwrap();
        let clone = avatars::get_avatar_with_config(&pool, clone_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(clone.owner, "bob.testnet");
        assert_eq!(clone.cloned_from, Some(source));
        assert_eq!(clone.config_id, config_id, "points to the strategy, no copy");
        assert!(!clone.listed, "F11.5: a clone is never listed");
        assert_eq!(clone.mode, "sim");
        assert_eq!(clone.starting_capital_usd, Decimal::from(10_000));
        assert_eq!(clone.cash_usd, Decimal::from(10_000));
        assert_eq!(clone.name, "Hanna (clone)");
        assert_eq!(clone.sprite, "boy");
        let persona_rows = sqlx::query_scalar!("SELECT count(*) FROM avatar_configs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(persona_rows, Some(1), "no new strategy row was written");
    }

    #[sqlx::test]
    async fn claiming_the_same_transaction_twice_creates_one_clone(pool: PgPool) {
        let (source, _) = seed_source(&pool).await;
        let (state, bob) =
            state_and_cookie(pool.clone(), "bob.testnet", tx_result(source, "bob.testnet")).await;

        let (first_status, first) = claim(&state, Some(&bob), TX_HASH).await;
        let (second_status, second) = claim(&state, Some(&bob), TX_HASH).await;

        assert_eq!(first_status, StatusCode::CREATED);
        assert_eq!(second_status, StatusCode::OK);
        assert_eq!(second["already_claimed"], true);
        assert_eq!(first["avatar_id"], second["avatar_id"]);
        assert_eq!(avatar_count(&pool).await, 2, "the source and one clone");
    }

    #[sqlx::test]
    async fn losing_a_race_returns_the_existing_clone_and_keeps_no_extra_row(pool: PgPool) {
        let (source, _) = seed_source(&pool).await;
        let (state, _) =
            state_and_cookie(pool.clone(), "bob.testnet", tx_result(source, "bob.testnet")).await;
        let purchase = parse_purchase(&tx_result(source, "bob.testnet"), CONTRACT).unwrap();

        // Two requests that both passed the "already claimed?" check.
        let (first, first_created) = create_clone(&state, TX_HASH, &purchase).await.unwrap();
        let (second, second_created) = create_clone(&state, TX_HASH, &purchase).await.unwrap();

        assert!(first_created);
        assert!(!second_created);
        assert_eq!(first, second);
        assert_eq!(avatar_count(&pool).await, 2, "the lost clone was rolled back");
    }

    #[sqlx::test]
    async fn nobody_can_claim_someone_elses_purchase(pool: PgPool) {
        let (source, _) = seed_source(&pool).await;
        // The chain says bob paid; mallory is the one logged in.
        let (state, mallory) = state_and_cookie(
            pool.clone(),
            "mallory.testnet",
            tx_result(source, "bob.testnet"),
        )
        .await;

        let (status, _) = claim(&state, Some(&mallory), TX_HASH).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(avatar_count(&pool).await, 1, "no clone was created");
    }

    #[sqlx::test]
    async fn claim_needs_a_login(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;

        let (status, _) = claim(&state, None, TX_HASH).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn a_bad_hash_an_unknown_transaction_or_a_foreign_event_is_a_400(pool: PgPool) {
        let (source, _) = seed_source(&pool).await;
        let mut foreign = tx_result(source, "bob.testnet");
        foreign["receipts_outcome"][0]["outcome"]["executor_id"] = json!("evil.testnet");
        let mut pending = tx_result(source, "bob.testnet");
        pending["final_execution_status"] = json!("INCLUDED");
        let unknown_source = tx_result(Uuid::from_u128(0xABCD), "bob.testnet");

        for (label, result, hash) in [
            ("bad hash", tx_result(source, "bob.testnet"), "x"),
            ("unknown transaction", Value::Null, OTHER_TX_HASH),
            ("foreign event", foreign, OTHER_TX_HASH),
            ("not final", pending, OTHER_TX_HASH),
            ("unknown character", unknown_source, OTHER_TX_HASH),
        ] {
            let (state, bob) = state_and_cookie(pool.clone(), "bob.testnet", result).await;
            let (status, _) = claim(&state, Some(&bob), hash).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{label}");
        }
        assert_eq!(avatar_count(&pool).await, 1, "nothing was created");
    }

    #[sqlx::test]
    async fn a_clone_cannot_be_bought(pool: PgPool) {
        let (source, config_id) = seed_source(&pool).await;
        let clone = avatars::insert_clone(
            &pool,
            &NewClone {
                owner: "creator.testnet".to_string(),
                source_id: source,
                config_id,
                name: "Copy".to_string(),
                sprite: "boy".to_string(),
                starting_capital_usd: Decimal::from(10_000),
            },
        )
        .await
        .unwrap();
        let (state, bob) =
            state_and_cookie(pool.clone(), "bob.testnet", tx_result(clone, "bob.testnet")).await;

        let (status, _) = claim(&state, Some(&bob), TX_HASH).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[sqlx::test]
    async fn listings_show_public_data_and_skip_unknown_characters(pool: PgPool) {
        let (source, _) = seed_source(&pool).await;
        let on_chain = json!([
            [source.to_string(), { "creator": "creator.testnet", "price": PRICE, "active": true }],
            [Uuid::from_u128(0xABCD).to_string(), { "creator": "x.testnet", "price": PRICE, "active": true }],
            ["not-a-uuid", { "creator": "x.testnet", "price": PRICE, "active": true }],
        ]);
        let bytes: Vec<u8> = on_chain.to_string().into_bytes();
        let state = state_with_rpc(
            pool,
            rpc_answer(json!({ "result": bytes, "logs": [], "block_height": 1 })),
        )
        .await;

        let (status, _, body) = send(
            router(state),
            request("GET", "/marketplace/listings", None, None),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let listings = body_json(&body);
        let listings = listings.as_array().unwrap();
        assert_eq!(listings.len(), 1);
        assert_eq!(listings[0]["avatar_id"], source.to_string());
        assert_eq!(listings[0]["name"], "Hanna");
        assert_eq!(listings[0]["risk"], "high");
        assert_eq!(listings[0]["creator"], "creator.testnet");
        assert_eq!(listings[0]["price_yocto"], PRICE);
        assert!(!String::from_utf8_lossy(&body).contains("Secret plan"));
    }
}
