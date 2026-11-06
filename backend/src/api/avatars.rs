//! Avatar endpoints (F3, F8.3).
//!
//! | Route | Who | Notes |
//! | --- | --- | --- |
//! | `GET /avatars?owner=` | anyone | summary per avatar, 2 queries for any count |
//! | `GET /avatars/{id}` | anyone | + risk, goal, cash, positions; persona only to its creator (D39) |
//! | `GET /avatars/{id}/decisions?source=&limit=` | anyone | newest first, one source |
//! | `GET /avatars/{id}/equity?source=` | anyone | oldest first, one source |
//! | `POST /avatars` | logged in | create + run the backtest (F3.3) |
//! | `PATCH /avatars/{id}` | owner | name, sprite, active, persona (D40) |
//!
//! **Persona privacy (D7, D39).** Only `AvatarDetailDto::persona` can carry
//! it, and it is filled only when the viewer created the strategy. A clone's
//! buyer owns the avatar but did not create the strategy, so they never see it.
//! The summary DTO has no persona field at all.

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::auth::AuthUser;
use super::{ApiError, AppState};
use crate::backtest;
use crate::db::avatars::{self, AvatarUpdate, AvatarWithConfig, NewAvatar, NewConfig};
use crate::db::{decisions, equity, positions};
use crate::live_events::main_village;

const MAX_NAME_CHARS: usize = 32;
/// The characters a player can pick (F3.1, D54). **Keep in sync** with
/// `frontend/src/game/sprites.ts`, which holds the art for each id.
pub const SPRITES: [&str; 8] = [
    "boy",
    "princess",
    "old_man",
    "hunter",
    "monk",
    "noble",
    "samurai",
    "ninja_blue",
];
const MAX_PERSONA_CHARS: usize = 2000;
const RISKS: [&str; 3] = ["low", "medium", "high"];
const GOALS: [&str; 3] = ["steady_income", "max_yield", "protect_capital"];
/// Sim starting capital options in USD (Q-E).
const STARTING_CAPITALS: [u64; 3] = [1_000, 10_000, 100_000];
/// `decisions.source` / `equity_snapshots.source` values; never mixed in one answer.
const SOURCES: [&str; 3] = ["backtest", "sim", "real"];
const DEFAULT_DECISIONS_LIMIT: i64 = 10;
const MAX_DECISIONS_LIMIT: i64 = 100;

/// Public view of an avatar (list and town map). No strategy fields.
#[derive(Debug, Serialize)]
pub struct AvatarSummaryDto {
    pub id: Uuid,
    pub owner: String,
    pub name: String,
    pub sprite: String,
    pub mode: String,
    pub active: bool,
    pub cloned_from: Option<Uuid>,
    pub listed: bool,
    pub created_at: DateTime<Utc>,
    /// Cash plus positions as last stored by the tick (not accrued since).
    pub equity_usd: Decimal,
    /// Where the sprite stands: the village holding the most money, `null`
    /// for all cash (TECH §8).
    pub main_village: Option<String>,
}

/// One avatar in detail (side panel, F8.3).
#[derive(Debug, Serialize)]
pub struct AvatarDetailDto {
    #[serde(flatten)]
    pub summary: AvatarSummaryDto,
    /// Public (D39): buyers need the risk label before cloning.
    pub risk: String,
    pub goal: String,
    pub starting_capital_usd: Decimal,
    pub cash_usd: Decimal,
    /// village id -> USD.
    pub positions: BTreeMap<String, Decimal>,
    /// Private strategy. Present only for the account that created it; the
    /// key is absent from the JSON for everyone else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EquityDto {
    pub ts: DateTime<Utc>,
    pub equity_usd: Decimal,
}

#[derive(Debug, Serialize)]
pub struct DecisionDto {
    pub id: i64,
    pub ts: DateTime<Utc>,
    pub source: String,
    pub input_hash: String,
    pub model: String,
    /// What the LLM asked for (`null` = no usable answer).
    pub proposed_action: Option<Value>,
    /// What was done (HOLD when refused).
    pub applied_action: Value,
    pub valid: bool,
    pub rejected_rule: Option<String>,
    pub reason_public: Option<String>,
    pub cost_usd: Decimal,
}

/// Answer of `POST /avatars`.
#[derive(Debug, Serialize)]
pub struct CreatedAvatarDto {
    pub avatar: AvatarDetailDto,
    /// The backtest track record, or `null` when the backtest failed (D41).
    pub backtest: Option<Vec<EquityDto>>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    owner: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SourceQuery {
    source: Option<String>,
    limit: Option<i64>,
}

/// Body of `POST /avatars` (F3.1). Unknown fields are refused, so a typo
/// cannot be silently ignored.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAvatarRequest {
    name: String,
    sprite: String,
    persona: String,
    risk: String,
    goal: String,
    mode: String,
    starting_capital_usd: u64,
}

/// Body of `PATCH /avatars/{id}`: only the fields to change. `risk` and
/// `goal` are not here on purpose (D40), so sending them is an error.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAvatarRequest {
    name: Option<String>,
    sprite: Option<String>,
    active: Option<bool>,
    persona: Option<String>,
}

/// `GET /avatars?owner=`: every avatar (all owners, or one owner).
pub async fn list_avatars(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<AvatarSummaryDto>>, ApiError> {
    let avatars = avatars::list_avatars(&state.db, query.owner.as_deref()).await?;
    let mut all_positions = positions::list_all_positions(&state.db).await?;
    let body = avatars
        .into_iter()
        .map(|avatar| {
            let held = all_positions.remove(&avatar.id).unwrap_or_default();
            summary(&avatar, &held)
        })
        .collect();
    Ok(Json(body))
}

/// `GET /avatars/{id}`.
pub async fn get_avatar(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    viewer: Option<AuthUser>,
) -> Result<Json<AvatarDetailDto>, ApiError> {
    let avatar = load_avatar(&state, id).await?;
    let held = positions::load_positions(&state.db, id).await?;
    Ok(Json(detail(avatar, held, viewer.as_ref())))
}

/// `GET /avatars/{id}/decisions?source=sim&limit=10`: newest first.
pub async fn list_decisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<SourceQuery>,
) -> Result<Json<Vec<DecisionDto>>, ApiError> {
    let source = required_source(query.source.as_deref())?;
    let limit = query.limit.unwrap_or(DEFAULT_DECISIONS_LIMIT);
    if !(1..=MAX_DECISIONS_LIMIT).contains(&limit) {
        return Err(ApiError::BadRequest(format!(
            "limit must be between 1 and {MAX_DECISIONS_LIMIT}"
        )));
    }
    load_avatar(&state, id).await?;

    let rows = decisions::list_decisions(&state.db, id, source, limit).await?;
    let body = rows
        .into_iter()
        .map(|row| DecisionDto {
            id: row.id,
            ts: row.ts,
            source: row.source,
            input_hash: row.input_hash,
            model: row.model,
            proposed_action: row.proposed_action,
            applied_action: row.applied_action,
            valid: row.valid,
            rejected_rule: row.rejected_rule,
            reason_public: row.reason_public,
            cost_usd: row.cost_usd,
        })
        .collect();
    Ok(Json(body))
}

/// `GET /avatars/{id}/equity?source=backtest`: oldest first.
pub async fn list_equity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<SourceQuery>,
) -> Result<Json<Vec<EquityDto>>, ApiError> {
    let source = required_source(query.source.as_deref())?;
    load_avatar(&state, id).await?;
    let points = equity::list_equity(&state.db, id, source).await?;
    Ok(Json(points.into_iter().map(equity_dto).collect()))
}

/// `POST /avatars`: creates the strategy and the avatar in one transaction,
/// then runs the 7-day backtest (F3.3) before answering.
///
/// The backtest runs after the commit: it reads the avatar back through the
/// pool. If it fails (e.g. no history yet), the avatar is kept and
/// `backtest` is `null` (D41).
pub async fn create_avatar(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<CreateAvatarRequest>,
) -> Result<(StatusCode, Json<CreatedAvatarDto>), ApiError> {
    let valid = validate_create(request).map_err(ApiError::BadRequest)?;

    // The user row exists: the session that authenticated `user` references it.
    let mut tx = state.db.begin().await?;
    let config_id = avatars::insert_config(
        &mut *tx,
        &NewConfig {
            persona: valid.persona,
            risk: valid.risk,
            goal: valid.goal,
            created_by: user.account_id.clone(),
        },
    )
    .await?;
    let id = avatars::insert_avatar(
        &mut *tx,
        &NewAvatar {
            owner: user.account_id.clone(),
            config_id,
            name: valid.name,
            sprite: valid.sprite,
            mode: valid.mode,
            starting_capital_usd: Decimal::from(valid.starting_capital_usd),
        },
    )
    .await?;
    tx.commit().await?;
    tracing::info!(avatar = %id, owner = %user.account_id, "avatar created");

    let backtest = match backtest::run_for_avatar(&state.db, id, Utc::now()).await {
        Ok(points) => Some(points.into_iter().map(equity_dto).collect()),
        Err(err) => {
            tracing::warn!(avatar = %id, "backtest on create failed: {err:#}");
            None
        }
    };
    let avatar = load_avatar(&state, id).await?;
    let body = CreatedAvatarDto {
        avatar: detail(avatar, BTreeMap::new(), Some(&user)),
        backtest,
    };
    Ok((StatusCode::CREATED, Json(body)))
}

/// `PATCH /avatars/{id}`: owner only (403 for anyone else).
///
/// A persona change inserts a **new** strategy row and points the avatar to
/// it (Q-B): the old row stays, so clones keep what they bought. Clones
/// cannot change the persona at all (they never saw it).
pub async fn update_avatar(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    user: AuthUser,
    Json(request): Json<UpdateAvatarRequest>,
) -> Result<Json<AvatarDetailDto>, ApiError> {
    let current = load_avatar(&state, id).await?;
    if current.owner != user.account_id {
        return Err(ApiError::Forbidden);
    }
    if request.persona.is_some() && current.cloned_from.is_some() {
        return Err(ApiError::Forbidden);
    }
    // Validate everything before writing anything.
    let name = request.name.as_deref().map(validate_name).transpose();
    let sprite = request.sprite.as_deref().map(validate_sprite).transpose();
    let persona = request.persona.as_deref().map(validate_persona).transpose();
    let (name, sprite, persona) = (
        name.map_err(ApiError::BadRequest)?,
        sprite.map_err(ApiError::BadRequest)?,
        persona.map_err(ApiError::BadRequest)?,
    );

    let mut tx = state.db.begin().await?;
    let config_id = match persona {
        Some(persona) => {
            avatars::insert_config(
                &mut *tx,
                &NewConfig {
                    persona,
                    risk: current.risk.clone(),
                    goal: current.goal.clone(),
                    created_by: user.account_id.clone(),
                },
            )
            .await?
        }
        None => current.config_id,
    };
    let update = AvatarUpdate {
        name: name.unwrap_or_else(|| current.name.clone()),
        sprite: sprite.unwrap_or_else(|| current.sprite.clone()),
        active: request.active.unwrap_or(current.active),
        config_id,
    };
    avatars::update_avatar(&mut *tx, id, &update).await?;
    tx.commit().await?;

    let avatar = load_avatar(&state, id).await?;
    let held = positions::load_positions(&state.db, id).await?;
    Ok(Json(detail(avatar, held, Some(&user))))
}

async fn load_avatar(state: &AppState, id: Uuid) -> Result<AvatarWithConfig, ApiError> {
    avatars::get_avatar_with_config(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)
}

fn summary(avatar: &AvatarWithConfig, held: &BTreeMap<String, Decimal>) -> AvatarSummaryDto {
    AvatarSummaryDto {
        id: avatar.id,
        owner: avatar.owner.clone(),
        name: avatar.name.clone(),
        sprite: avatar.sprite.clone(),
        mode: avatar.mode.clone(),
        active: avatar.active,
        cloned_from: avatar.cloned_from,
        listed: avatar.listed,
        created_at: avatar.created_at,
        equity_usd: avatar.cash_usd + held.values().sum::<Decimal>(),
        main_village: main_village(held),
    }
}

/// The persona is copied only when `viewer` created the strategy (D39).
fn detail(
    avatar: AvatarWithConfig,
    held: BTreeMap<String, Decimal>,
    viewer: Option<&AuthUser>,
) -> AvatarDetailDto {
    let is_creator = viewer.is_some_and(|user| user.account_id == avatar.config_created_by);
    AvatarDetailDto {
        summary: summary(&avatar, &held),
        persona: is_creator.then_some(avatar.persona),
        risk: avatar.risk,
        goal: avatar.goal,
        starting_capital_usd: avatar.starting_capital_usd,
        cash_usd: avatar.cash_usd,
        positions: held,
    }
}

fn equity_dto(point: equity::EquityPoint) -> EquityDto {
    EquityDto {
        ts: point.ts,
        equity_usd: point.equity_usd,
    }
}

/// `source` is required: backtest, sim and real are never merged (CLAUDE.md
/// "data source labels"), so the client must say which one it wants.
fn required_source(source: Option<&str>) -> Result<&'static str, ApiError> {
    let wanted = source.unwrap_or_default();
    SOURCES
        .into_iter()
        .find(|known| *known == wanted)
        .ok_or_else(|| {
            ApiError::BadRequest("source is required: backtest, sim or real".to_string())
        })
}

/// A create request after validation (names trimmed).
#[derive(Debug, PartialEq, Eq)]
struct ValidCreate {
    name: String,
    sprite: String,
    persona: String,
    risk: String,
    goal: String,
    mode: String,
    starting_capital_usd: u64,
}

/// Checks a create request. Pure, so every rule is unit-tested. The
/// database CHECKs repeat the main ones (design doc C9).
fn validate_create(request: CreateAvatarRequest) -> Result<ValidCreate, String> {
    let name = validate_name(&request.name)?;
    let sprite = validate_sprite(&request.sprite)?;
    let persona = validate_persona(&request.persona)?;
    if !RISKS.contains(&request.risk.as_str()) {
        return Err("risk must be low, medium or high".to_string());
    }
    if !GOALS.contains(&request.goal.as_str()) {
        return Err("goal must be steady_income, max_yield or protect_capital".to_string());
    }
    match request.mode.as_str() {
        "sim" => {}
        // D42: real mode arrives with the vault (Day 10).
        "real" => return Err("real mode is not available yet".to_string()),
        _ => return Err("mode must be sim".to_string()),
    }
    if !STARTING_CAPITALS.contains(&request.starting_capital_usd) {
        return Err("starting_capital_usd must be 1000, 10000 or 100000".to_string());
    }
    Ok(ValidCreate {
        name,
        sprite,
        persona,
        risk: request.risk,
        goal: request.goal,
        mode: request.mode,
        starting_capital_usd: request.starting_capital_usd,
    })
}

/// 1 to 32 characters after trimming spaces. Returns the trimmed name.
fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let chars = name.chars().count();
    if chars == 0 || chars > MAX_NAME_CHARS {
        return Err(format!("name must be 1 to {MAX_NAME_CHARS} characters"));
    }
    Ok(name.to_string())
}

/// Must be one of [`SPRITES`] (F3.1, D54).
fn validate_sprite(sprite: &str) -> Result<String, String> {
    if !SPRITES.contains(&sprite) {
        return Err(format!("sprite must be one of: {}", SPRITES.join(", ")));
    }
    Ok(sprite.to_string())
}

/// 1 to 2000 characters (counted like Postgres `char_length`), not only spaces.
fn validate_persona(persona: &str) -> Result<String, String> {
    if persona.trim().is_empty() || persona.chars().count() > MAX_PERSONA_CHARS {
        return Err(format!(
            "persona must be 1 to {MAX_PERSONA_CHARS} characters"
        ));
    }
    Ok(persona.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::auth::tests::{login_cookie, state_with_rpc};
    use crate::api::near_rpc::tests::full_access_answer;
    use crate::api::router;
    use crate::api::tests::{json, request, send};
    use crate::db::{villages, yield_snapshots};
    use chrono::Days;
    use serde_json::json;
    use sqlx::PgPool;

    const PERSONA: &str = "Secret plan: always chase the highest lending rate on Tuesdays.";

    fn create_body() -> Value {
        json!({
            "name": "  Hanna  ",
            "sprite": "boy",
            "persona": PERSONA,
            "risk": "high",
            "goal": "max_yield",
            "mode": "sim",
            "starting_capital_usd": 10000
        })
    }

    fn create_request(body: Value) -> CreateAvatarRequest {
        serde_json::from_value(body).unwrap()
    }

    /// 10 days of daily points for every village, so the backtest can run.
    async fn seed_history(pool: &PgPool) {
        let today = Utc::now().date_naive();
        for village in villages::list_active_villages(pool).await.unwrap() {
            for days_ago in 1..=10u64 {
                let day = today.checked_sub_days(Days::new(days_ago)).unwrap();
                yield_snapshots::insert_snapshot(
                    pool,
                    &yield_snapshots::YieldSnapshot {
                        village_id: village.id.clone(),
                        ts: day.and_hms_opt(12, 0, 0).unwrap().and_utc(),
                        apy_base: Decimal::TEN,
                        apy_reward: Decimal::ZERO,
                        tvl_usd: Decimal::from(1_000_000),
                    },
                )
                .await
                .unwrap();
            }
        }
    }

    /// Logs in `account` and creates an avatar for it. Returns (cookie, id).
    async fn create_as(state: &AppState, account: &str) -> (String, Uuid) {
        let cookie = login_cookie(state, account).await;
        let (status, _, body) = send(
            router(state.clone()),
            request("POST", "/avatars", Some(&cookie), Some(create_body())),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "{}",
            String::from_utf8_lossy(&body)
        );
        let id = json(&body)["avatar"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        (cookie, id)
    }

    async fn get_json(state: &AppState, uri: &str, cookie: Option<&str>) -> (StatusCode, Value) {
        let (status, _, body) =
            send(router(state.clone()), request("GET", uri, cookie, None)).await;
        let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        (status, value)
    }

    async fn patch(state: &AppState, id: Uuid, cookie: &str, body: Value) -> (StatusCode, Value) {
        let (status, _, body) = send(
            router(state.clone()),
            request("PATCH", &format!("/avatars/{id}"), Some(cookie), Some(body)),
        )
        .await;
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    // ---- validation (pure) ----

    #[test]
    fn a_valid_create_request_is_accepted_and_the_name_trimmed() {
        let valid = validate_create(create_request(create_body())).unwrap();
        assert_eq!(valid.name, "Hanna");
        assert_eq!(valid.starting_capital_usd, 10_000);
    }

    #[test]
    fn each_invalid_create_field_is_refused() {
        for (field, bad) in [
            ("name", json!("   ")),
            ("name", json!("x".repeat(33))),
            ("sprite", json!("")),
            ("sprite", json!("Merchant 1")),
            ("sprite", json!("merchant_1")),
            ("sprite", json!("Boy")),
            ("persona", json!("  ")),
            ("persona", json!("a".repeat(2001))),
            ("risk", json!("yolo")),
            ("goal", json!("moon")),
            ("mode", json!("real")),
            ("mode", json!("paper")),
            ("starting_capital_usd", json!(5000)),
        ] {
            let mut body = create_body();
            body[field] = bad.clone();
            assert!(
                validate_create(create_request(body)).is_err(),
                "{field} = {bad}"
            );
        }
    }

    #[test]
    fn every_listed_sprite_is_accepted() {
        for sprite in SPRITES {
            assert_eq!(validate_sprite(sprite).as_deref(), Ok(sprite));
        }
    }

    #[test]
    fn real_mode_says_it_is_not_available_yet() {
        let mut body = create_body();
        body["mode"] = json!("real");
        let err = validate_create(create_request(body)).unwrap_err();
        assert!(err.contains("not available yet"));
    }

    #[test]
    fn persona_limit_counts_characters_not_bytes() {
        assert!(validate_persona(&"é".repeat(2000)).is_ok());
        assert!(validate_persona(&"é".repeat(2001)).is_err());
    }

    // ---- create ----

    #[sqlx::test]
    async fn create_needs_a_login(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;

        let (status, _, _) = send(
            router(state),
            request("POST", "/avatars", None, Some(create_body())),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test]
    async fn create_stores_the_avatar_and_its_backtest(pool: PgPool) {
        seed_history(&pool).await;
        let state = state_with_rpc(pool.clone(), full_access_answer()).await;
        let cookie = login_cookie(&state, "alice.testnet").await;

        let (status, _, body) = send(
            router(state.clone()),
            request("POST", "/avatars", Some(&cookie), Some(create_body())),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        let body = json(&body);
        assert_eq!(body["avatar"]["name"], "Hanna");
        assert_eq!(body["avatar"]["owner"], "alice.testnet");
        assert_eq!(body["avatar"]["cash_usd"], "10000");
        assert_eq!(body["avatar"]["persona"], PERSONA, "creator sees it");
        let backtest = body["backtest"].as_array().unwrap();
        assert_eq!(backtest.len(), 7, "one point per day of the 7-day window");

        let id: Uuid = body["avatar"]["id"].as_str().unwrap().parse().unwrap();
        let stored = equity::list_equity(&pool, id, "backtest").await.unwrap();
        assert_eq!(stored.len(), 7);
    }

    #[sqlx::test]
    async fn create_without_history_keeps_the_avatar_with_a_null_backtest(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let cookie = login_cookie(&state, "alice.testnet").await;

        let (status, _, body) = send(
            router(state.clone()),
            request("POST", "/avatars", Some(&cookie), Some(create_body())),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        let body = json(&body);
        assert!(body["backtest"].is_null());
        let (_, list) = get_json(&state, "/avatars", None).await;
        assert_eq!(list.as_array().unwrap().len(), 1);
    }

    #[sqlx::test]
    async fn create_with_an_invalid_field_is_a_400(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let cookie = login_cookie(&state, "alice.testnet").await;
        let mut body = create_body();
        body["starting_capital_usd"] = json!(7);

        let (status, _, response) = send(
            router(state),
            request("POST", "/avatars", Some(&cookie), Some(body)),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            json(&response)["error"]
                .as_str()
                .unwrap()
                .contains("starting_capital_usd")
        );
    }

    // ---- read + persona privacy ----

    #[sqlx::test]
    async fn persona_is_shown_only_to_its_creator(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let (alice, id) = create_as(&state, "alice.testnet").await;
        let bob = login_cookie(&state, "bob.testnet").await;
        let uri = format!("/avatars/{id}");

        let (_, as_alice) = get_json(&state, &uri, Some(&alice)).await;
        let (_, as_bob) = get_json(&state, &uri, Some(&bob)).await;
        let (_, anonymous) = get_json(&state, &uri, None).await;

        assert_eq!(as_alice["persona"], PERSONA);
        assert!(as_bob.get("persona").is_none(), "{as_bob}");
        assert!(anonymous.get("persona").is_none(), "{anonymous}");
        assert_eq!(anonymous["risk"], "high", "risk is public (D39)");
        assert!(!anonymous.to_string().contains("Secret plan"));
    }

    #[sqlx::test]
    async fn a_clone_owner_does_not_see_the_persona(pool: PgPool) {
        let state = state_with_rpc(pool.clone(), full_access_answer()).await;
        let (_, source) = create_as(&state, "alice.testnet").await;
        let bob = login_cookie(&state, "bob.testnet").await;
        // Clone creation is Day 9: shape the clone by hand. It points to
        // alice's strategy and belongs to bob.
        let config_id = avatars::get_avatar_with_config(&pool, source)
            .await
            .unwrap()
            .unwrap()
            .config_id;
        let clone = avatars::insert_avatar(
            &pool,
            &NewAvatar {
                owner: "bob.testnet".to_string(),
                config_id,
                name: "Copy".to_string(),
                sprite: "merchant_2".to_string(),
                mode: "sim".to_string(),
                starting_capital_usd: Decimal::from(1000),
            },
        )
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE avatars SET cloned_from = $1 WHERE id = $2",
            source,
            clone
        )
        .execute(&pool)
        .await
        .unwrap();

        let (_, as_owner) = get_json(&state, &format!("/avatars/{clone}"), Some(&bob)).await;
        assert!(as_owner.get("persona").is_none(), "{as_owner}");

        // And bob cannot set a persona on the clone either.
        let (status, _) = patch(&state, clone, &bob, json!({ "persona": "mine now" })).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[sqlx::test]
    async fn list_shows_summaries_without_any_persona(pool: PgPool) {
        let state = state_with_rpc(pool.clone(), full_access_answer()).await;
        let (alice, id) = create_as(&state, "alice.testnet").await;
        create_as(&state, "bob.testnet").await;
        let mut conn = pool.acquire().await.unwrap();
        let held: BTreeMap<String, Decimal> = [
            ("rhea-lend-usdc".to_string(), Decimal::from(3000)),
            ("rhea-lend-usdt".to_string(), Decimal::from(1000)),
        ]
        .into();
        positions::replace_positions(&mut conn, id, &held)
            .await
            .unwrap();
        avatars::update_cash(&pool, id, Decimal::from(6000))
            .await
            .unwrap();

        // Even logged in as the creator, the list never carries the persona.
        let (status, list) = get_json(&state, "/avatars", Some(&alice)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list.as_array().unwrap().len(), 2);
        assert!(!list.to_string().contains("persona"));
        assert!(!list.to_string().contains("Secret plan"));

        let (_, mine) = get_json(&state, "/avatars?owner=alice.testnet", None).await;
        let mine = mine.as_array().unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0]["equity_usd"], "10000");
        assert_eq!(mine[0]["main_village"], "rhea-lend-usdc");
    }

    #[sqlx::test]
    async fn unknown_avatar_is_a_404(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let uri = format!("/avatars/{}", Uuid::nil());

        assert_eq!(get_json(&state, &uri, None).await.0, StatusCode::NOT_FOUND);
        assert_eq!(
            get_json(&state, &format!("{uri}/equity?source=sim"), None)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }

    // ---- decisions + equity ----

    #[sqlx::test]
    async fn decisions_and_equity_need_one_source_and_never_mix_them(pool: PgPool) {
        seed_history(&pool).await;
        let state = state_with_rpc(pool.clone(), full_access_answer()).await;
        let (_, id) = create_as(&state, "alice.testnet").await;
        equity::insert_equity_point(
            &pool,
            id,
            "sim",
            &equity::EquityPoint {
                ts: Utc::now(),
                equity_usd: Decimal::from(10_001),
            },
        )
        .await
        .unwrap();

        let (status, _) = get_json(&state, &format!("/avatars/{id}/equity"), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "source is required");
        let (status, _) =
            get_json(&state, &format!("/avatars/{id}/equity?source=paper"), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (_, sim) = get_json(&state, &format!("/avatars/{id}/equity?source=sim"), None).await;
        let (_, backtest) = get_json(
            &state,
            &format!("/avatars/{id}/equity?source=backtest"),
            None,
        )
        .await;
        assert_eq!(sim.as_array().unwrap().len(), 1);
        assert_eq!(backtest.as_array().unwrap().len(), 7);

        let decisions_uri = format!("/avatars/{id}/decisions");
        assert_eq!(
            get_json(&state, &decisions_uri, None).await.0,
            StatusCode::BAD_REQUEST
        );
        for limit in [0, 101] {
            let uri = format!("{decisions_uri}?source=sim&limit={limit}");
            assert_eq!(
                get_json(&state, &uri, None).await.0,
                StatusCode::BAD_REQUEST
            );
        }
        let (status, rows) = get_json(&state, &format!("{decisions_uri}?source=sim"), None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(rows.as_array().unwrap().is_empty(), "no sim decisions yet");
    }

    // ---- update ----

    #[sqlx::test]
    async fn only_the_owner_can_update(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let (alice, id) = create_as(&state, "alice.testnet").await;
        let bob = login_cookie(&state, "bob.testnet").await;

        let (status, _) = patch(&state, id, &bob, json!({ "name": "Stolen" })).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, body) = patch(
            &state,
            id,
            &alice,
            json!({ "name": "Yusra", "sprite": "samurai", "active": false }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["name"], "Yusra");
        assert_eq!(body["sprite"], "samurai");
        assert_eq!(body["active"], false);
    }

    #[sqlx::test]
    async fn a_persona_change_creates_a_new_strategy_row(pool: PgPool) {
        let state = state_with_rpc(pool.clone(), full_access_answer()).await;
        let (alice, id) = create_as(&state, "alice.testnet").await;
        let old_config = avatars::get_avatar_with_config(&pool, id)
            .await
            .unwrap()
            .unwrap()
            .config_id;

        let (status, body) = patch(&state, id, &alice, json!({ "persona": "New plan." })).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["persona"], "New plan.");
        assert_eq!(body["risk"], "high", "risk is kept");
        let now = avatars::get_avatar_with_config(&pool, id)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(now.config_id, old_config);
        let old_persona = sqlx::query_scalar!(
            "SELECT persona FROM avatar_configs WHERE id = $1",
            old_config
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            old_persona, PERSONA,
            "the old strategy row is unchanged (Q-B)"
        );
    }

    #[sqlx::test]
    async fn risk_cannot_be_patched_and_bad_values_are_refused(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let (alice, id) = create_as(&state, "alice.testnet").await;

        let (status, _) = patch(&state, id, &alice, json!({ "risk": "low" })).await;
        assert!(
            status.is_client_error(),
            "unknown field refused, got {status}"
        );
        let (status, _) = patch(&state, id, &alice, json!({ "name": "" })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = patch(&state, id, &alice, json!({ "persona": " " })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[sqlx::test]
    async fn patch_needs_a_login(pool: PgPool) {
        let state = state_with_rpc(pool, full_access_answer()).await;
        let (_, id) = create_as(&state, "alice.testnet").await;

        let (status, _, _) = send(
            router(state),
            request(
                "PATCH",
                &format!("/avatars/{id}"),
                None,
                Some(json!({ "name": "x" })),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
