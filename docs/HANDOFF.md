# Fondouk: Handoff

Last updated: 6 Nov 2026. Update this file at the end of every work session.

This file is for anyone joining the project (teammates or a coding agent). Read it first, then `overview.md`, `SPEC.md`, `TECH_architecture.md`.

---

## 1. Current state

- **Phase:** Day 1 to Day 6 committed, **Day 7 built 6 Nov (not committed)**. The backtest is **optional** (P2, D51): D35 is on hold. Next: pick the LLM provider for the long-running seed (see U5), run `scripts/seed_demo.sql`, then Day 8 (marketplace contract).
- **Day 7 town (6 Nov):** Ninja Adventure art (CC0, D52) in `frontend/public/assets/`. Ground = tile array built in code (D53, `game/townMap.ts`), houses cropped from the house tileset, 8 pickable characters (D54, list in backend `api::avatars::SPRITES` and `game/sprites.ts`). Sprites stand at `main_village` (queue in front of the house, all-cash ones in the arena) and walk village → arena centre → village on `avatar_move` (~3 s, `game/walkPath.ts`). Click a sprite or a house: side panel (F8.3 avatar: owner, mode badge, equity chart, positions, last 10 decisions with `reason_public`; F8.4 village: 30-day APY chart, TVL, characters inside = `main_village`). Panels reload on live events. Demo seed script `scripts/seed_demo.sql` (10 characters, D55). Checked in Chrome: map, sprite, both panels, no console errors. **Not yet seen: a sprite walking** (needs a working LLM, see U5). The Day 6 real-wallet login works (a `buoya125.testnet` session was active). 36 frontend tests, 254 backend tests.
- **Day 6 frontend (6 Nov):** `frontend/` = Vite + React + TypeScript, Phaser 4, NEAR Connect (D46). Town map drawn in code (placeholder until the Day 7 tileset, D45): 4 villages with live APY label, 7-day sparkline and "stale · N h ago" badge, roads and an empty arena. Data: REST through the `/api` dev proxy (D47), live `apr_update` over `/ws` with a 5 s reconnect (D49), React → Phaser through a Phaser EventEmitter (D48). Login button runs the NEP-413 flow against `/auth/*`. 13 vitest tests (D50), lint and build clean. **Not yet tested by hand: a real wallet login and the map in a browser with the backend running.**
- **Day 5 API (6 Nov):** NEP-413 wallet login with HttpOnly cookie sessions (D37, D38: `/auth/challenge`, `/auth/near`, `/auth/logout`, `/auth/me`), avatar endpoints (`GET /avatars`, `/avatars/{id}`, `…/decisions`, `…/equity`, `POST /avatars` with backtest, `PATCH /avatars/{id}`, D39–D42), `GET /villages/{id}/history`, and `/ws` live events (`apr_update`, `decision`, `avatar_move`, D43). Migration 0007 (`auth_challenges`, `sessions`). 253 tests pass. Login was only tested with locally generated keys: **a real NEAR Connect signature is first tested on Day 6.**
- **What runs today:** `backend/` (axum + sqlx + Postgres). DefiLlama indexer for the 4 villages (D13, D14), fetching every hour (D19); it stores the full chart, so about 45 days of history are imported (D22). `GET /health`, `GET /villages` (latest yield + stale flag). Tables for users, strategies, avatars, positions, equity, decisions and LLM cache. Cost model (`sim::costs`, D31): pool fee + slippage + gas per step, and a stablecoin swap through the stable pool when the target does not accept the token. Guardrails (`agent::guardrails`, all of SPEC §6, including the 2 cost rules, D32) checked on every action. LLM decision pipeline (`agent::decide`: prompt, OpenAI-compatible call, parse, retry once, HOLD, cache). Agent tick loop (`agent::tick`, every 10 min, only when `LLM_*` is set): per sim avatar it accrues yield for the time since its last tick (D33), decides, moves the paper money paying the cost, and stores positions, cash, the decision and a `sim` equity point in one transaction (D34). Backtest engine pays the same costs, but still uses the mock decision rule (all-in on the best APY, so only `high` risk avatars invest with it). The backtest is optional (D51); the LLM switch (D35) is on hold. 189 tests pass (the LLM is faked by a local test server). The real LLM call is **not tested yet** (no key, U5/U6). No frontend or contracts yet.
- **Product code started:** 1 Nov 2026 (hackathon start).
- **Deadline:** 14 Nov 2026, 23:59 UTC. Repo public + demo post on X + one submission.
- **Team:** Ayoub (lead, full-stack, Rust and contracts), starting solo. Teammates join later.

### Note on pre-hackathon work
The official page asks teams to build the project during the hackathon. We did not find a rule that bans earlier research, but to be safe: before 1 Nov we only do docs, research, and throwaway spikes (data checks, API tests). Product code is written from 1 Nov in the public repo history.

---

## 2. Decisions made (and why)

| # | Decision | Why |
| --- | --- | --- |
| D1 | Name: **Fondouk** | A fondouk was the merchants' inn in the Tunis medina: traders stayed, traded and shared market news. Maps to agents, arena, marketplace and future cross-chain "caravans". |
| D2 | Primary track: `games`. Also `agents`, `security`, `privacy`, maybe `defi`. | The games track asks for NPCs with wallets and player-owned markets. That is our core. |
| D3 | NEAR-only villages for MVP. Cross-chain via NEAR Intents is a next step. | NEAR DeFi is mostly Rhea, but scope must fit 14 days. Cross-chain is mentioned in the pitch. |
| D4 | Simulation mode with live data is the main mode. Real mode is P1 with tiny caps. | Lower risk, faster demo, still uses real data. |
| D5 | ~~30-day backtest on character creation.~~ **Superseded by D51** (backtest is optional). | Without it, there is no 7d or 30d history at demo time (the hackathon is 14 days). D51 answers this with an early live seed. |
| D6 | LLM picks from a fixed action set. Guardrails in code. | LLM output is not reliable enough to move money freely. |
| D7 | Strategy (persona) is hidden. Clones reference the config, never copy the text. | If the strategy is visible, nobody pays to clone. |
| D8 | Clone payment: one-time fee in NEAR, 90% creator, 10% platform. Profit share is a next step. | Simple, shows a working on-chain payment. |
| D9 | Vault contract with operator function-call key. | The AI key can only call vault operator methods. It cannot send funds out. Fits the `security` track. |
| D10 | Stack: Rust (axum, sqlx, near-api, async-openai), near-sdk-rs contracts, React + Vite + Phaser 4 frontend, NEAR AI Cloud for inference. | Team strength in Rust and Postgres. Phaser saves time on the map. NEAR AI Cloud fits the `privacy` track. |
| D11 | Modular monolith backend. | One binary is faster to build and debug solo. |
| D12 | Shade Agent Framework is **not** used. | NEAR docs say it is no longer maintained as standalone tooling after 19 Apr 2026. |
| D13 | (2 Nov) Live indexer and backtest both use DefiLlama yields. Rhea on-chain reads are optional later. | DEX pool APR needs volume, which view calls do not give. One source keeps live and backtest data in the same unit (APY). |
| D14 | (2 Nov) 4 villages: Rhea Lend USDC, Rhea Lend USDT, DEX USDT-USDC-USDT.e-USDC.e, DEX FRAX-USDC (pool ids in U1). | The only set that meets SPEC F2.1 (2 lending + 2 DEX). |
| D15 | (2 Nov) Stale = the indexer has not fetched the village successfully for 30 min (threshold replaced by D20). Snapshot columns renamed to `apy_base` / `apy_reward`. | DefiLlama refreshes about hourly, so the source timestamp would always look stale. Column names must match the unit we store. |
| D16 | (2 Nov) LLM client: async-openai instead of rig. | Ayoub's choice: a plain OpenAI-compatible client is enough for one chat call per tick. |
| D17 | (2 Nov) LLM provider is chosen by env (`LLM_BASE_URL`, `LLM_API_KEY`, `LLM_MODEL`). OpenRouter for development, NEAR AI Cloud for the demo and submission. | Use existing OpenRouter credits during dev. NEAR AI Cloud (TEE private inference) keeps the `privacy` track claim true for the demo. The model id must be part of the backtest decision cache key. |
| D18 | (2 Nov) Database design approved: `dev-docs/2026-11-02/database-design.md`. One persona field (max 2000 chars), immutable strategy configs that hold risk and goal, sim capital $1k/$10k/$100k starting in cash, server-side sessions, listing cached in DB. | Clones must keep exactly the strategy they paid for. Answers to Q-A to Q-H. |
| D19 | (2 Nov) Indexer fetch interval is a setting, default 1 hour (may become 24 hours later). | DefiLlama refreshes about hourly. Stale threshold must follow it (see D20). |
| D20 | (2 Nov) Stale threshold is a setting, `VILLAGE_STALE_AFTER_SECS`, default 2 x the fetch interval (2 hours), and it must be longer than the interval. Rejected: a fixed value. | A 30-minute threshold with an hourly fetch would make every village stale half of the time. One missed fetch is tolerated, two in a row make the village stale. |
| D21 | (4 Nov) Yield accrues from APY with exact daily compounding: `value * (1 + apy/100)^(1/365)` per day, using the `maths` feature of `rust_decimal`. Rejected: simple `apy/365` (overstates about 1 point per year at 14%), converting APY to APR first (same math, extra step). | We store APY (D13); SPEC F6.2's APR formula would overstate returns. |
| D22 | (4 Nov) History import = the indexer stores **every** point of the DefiLlama chart each round (`ON CONFLICT DO NOTHING`). Points with a null `apyBase` are skipped. Rejected: a separate one-time import at startup. | One code path; gaps after downtime fill themselves. |
| D23 | (4 Nov) A village with no point for a day: money already there keeps its last known APY, but it cannot receive new funds that day. | Same spirit as the stale rule; gaps are rare. |
| D24 | (4 Nov) Re-running a backtest replaces the avatar's `backtest` equity rows in one transaction. Rejected: failing on the second run. | Safe re-runs, never half a track record. |
| D25 | (4 Nov) `equity_snapshots.equity_usd` has `CHECK (equity_usd >= 0)` (migration 0004, now also in the design doc). Rejected: dropping it with a new migration. | Paper equity cannot go negative (no leverage or borrowing), so a negative value can only be a ledger bug. The DB rejects it loudly. |
| D26 | (4 Nov) The `backend/.sqlx/` query cache is committed. Run `cargo sqlx prepare -- --all-targets` after changing any `query!` SQL. Rejected: keeping it ignored. | Teammates, CI and the deploy build can compile with `SQLX_OFFLINE=true` and no live database. |
| D27 | (5 Nov) MOVE goes from village to village only. Cash is not a village and is invested only with REBALANCE. Rejected: `"cash"` as a pseudo-village in MOVE; investing cash automatically on the first tick. | No new concept: `Ledger::rebalance` already empties cash. |
| D28 | (5 Nov) The 2 cost-based guardrails ("min APR gain after costs", "cost larger than 7 days of gain") are built on Day 4 with the cost model. | They need costs; writing a fake version now means writing them twice. |
| D29 | (5 Nov) "Max percent moved per tick" is a share of **equity**, and counts only money taken **out of villages**. Investing cash does not count (it still counts for max share, and it starts the cooldown). | Otherwise a new low-risk avatar (25% per move, 6-tick cooldown) would sit mostly in cash for about 18 ticks, which is 18 of the 30 backtest days. |
| D30 | (5 Nov) Day 3 tick loop only **logs** decisions (`decisions`, `source = 'sim'`). Positions and moving paper money live come on Day 4 with the `positions` table. | Keeps Day 3 small; the logged rows already meet the F4 acceptance. |
| D31 | (6 Nov) Cost model is **kind-aware** (`sim::costs`). A move = exit source (lending: gas; LP: fee + slippage + gas) → swap through `rhea-dex-stable4` only if the target does not accept the token in hand (fee + slippage + gas; swap pool stale → `no_swap_route`) → enter target (lending: gas; LP: fee + slippage + gas). Slippage = `amount × 0.1 × amount / TVL`. Gas $0.02 per transaction (about 0.005 NEAR at $3.66). Cash is USDC. Fees and tokens live in `villages.fee_bps` / `villages.tokens` (migration 0006): Rhea pools 4179 and 4514 = 2 bps, Rhea Lend = 0 (checked on-chain 6 Nov). Rejected: one uniform cost per leg (simpler, less realistic); fee data as Rust constants (village data in two places). | Moves cost what they would on Rhea, including USDC↔USDT swaps. Errs on the expensive side (stable pools have less price impact than linear slippage). |
| D32 | (6 Nov) "Min APR gain after costs" spreads the cost over **30 days**, then annualizes it: `gain − cost/moved × 100 × 365/30 ≥ min`. Gain is measured on the money that moves (APY where it goes minus where it comes from; cash and unquoted villages = 0%). Rejected: 7 days (nearly the same as "cost > 7 days of gain"), no cost subtraction. | Same window as the backtest; the two cost rules stay different. |
| D33 | (6 Nov) Live accrual covers the **real time** since the avatar's last `sim` equity point, at each village's latest APY (stale ones included). Rejected: a fixed `AGENT_TICK_SECS` per tick. | Money in a pool keeps earning while our server is down. No new column: the last equity row is the clock. |
| D34 | (6 Nov) One `sim` equity point per avatar **every tick**, written in the same transaction as positions, cash and the decision. Rejected: hourly points. | Simplest; about 15k rows for 10 avatars by demo day. |
| D35 | (6 Nov) Switching the backtest from the mock rule to the LLM is a **separate plan right after Day 4**. Rejected: folding it into Day 4. **On hold (D51):** only if time. | Keeps the Day 4 change reviewable; it changes `backtest::run` to async. |
| D36 | (6 Nov) Day 5 (API) is built **before** D35; D35 comes right after. Rejected: D35 first. | Ayoub asked for Day 5; the two do not clash (`POST /avatars` calls `run_for_avatar`, whose inside D35 changes). |
| D37 | (6 Nov) Session = **HttpOnly cookie** `fondouk_session` (`Secure`, `SameSite=Lax`, 7 days), sha256 of the token stored in `sessions`. Frontend and REST API must be same-origin (Vite proxy in dev, Vercel rewrite in prod). `/ws` is public and connects directly. Rejected: `Authorization: Bearer` token (readable by JS, needs a CORS layer). | Matches the approved design (server-side, revocable); the cookie cannot be stolen by injected JS and Lax blocks cross-site POSTs. |
| D38 | (6 Nov) Login = NEP-413: `POST /auth/challenge` (32-byte nonce, 5 min), `POST /auth/near` consumes the nonce first (atomic `DELETE … RETURNING`), verifies the ed25519 signature over `sha256(u32 2^31+413 ‖ borsh payload)`, recipient `fondouk`, and requires the key to be a **FullAccess** key of the account (RPC `view_access_key`, `NEAR_RPC_URL`). Borsh and cookie parsing written by hand. | Single-use nonce = no replay; a function-call key left in a browser does not prove account ownership. No DB transaction held during the RPC call. |
| D39 | (6 Nov) Avatar `risk` and `goal` are public. `persona` is returned only when the viewer created the config (`avatar_configs.created_by`), so a clone buyer never sees it. | Buyers need the risk label before cloning; the persona stays the secret (D7). |
| D40 | (6 Nov) `PATCH /avatars/:id` (owner only) can change `name`, `sprite`, `active`, `persona`. A persona edit inserts a new config row (Q-B) and is forbidden for clones. `risk` / `goal` are not editable. | Clones keep what they bought; no hidden strategy changes. |
| D41 | (6 Nov) If the backtest fails on create (e.g. no history), the avatar is kept and the answer is 201 with `backtest: null`. Rejected: failing the whole request. | A data gap must not block character creation; the error is logged. |
| D42 | (6 Nov) `POST /avatars` with `mode: "real"` returns 400 until real mode exists (Day 10). Sprite = 1–32 chars `[a-z0-9_-]` until the sprite list exists (Day 7). | No inert real avatars; no invented sprite names. |
| D43 | (6 Nov) Live events go through one `tokio::sync::broadcast` channel (`live_events::LiveEvent`): `apr_update` after each indexer round, `decision` and `avatar_move` after each avatar's tick commit. Public data only, never the persona. `avatar_move` = the village holding the most money changed (`null` = all cash). | Every browser gets a copy; a slow browser loses old events instead of slowing the tick. |
| D44 | (6 Nov) Day 6 (frontend shell) is built **before** D35; D35 follows. Rejected: D35 first (as D36 planned). | Ayoub asked for Day 6; frontend and `backtest::run` do not touch each other. ~~D35 must land before the Day 11 create-avatar flow.~~ No longer true (D51). |
| D45 | (6 Nov) Day 6 map is **drawn in code** (Phaser Graphics: roads, arena, coloured buildings). The real tileset, Tiled map and sprites come together on Day 7. Village positions stay the DB `map_x/map_y` (0–400 grid, drawn ×2 on an 800×800 canvas), no migration. Rejected: picking a tileset and drawing a Tiled map on Day 6. | Art is picked once with the sprites; the placeholder already meets "map with live APR labels". |
| D46 | (6 Nov) Wallet = `@hot-labs/near-connect` used directly (`frontend/src/lib/wallet.ts`). Login = `/auth/challenge` → `connector.connect({ signMessageParams })` (pick wallet + sign in + sign, one popup; `wallet.signMessage` if already connected) → `/auth/near`. Rejected: `near-connect-hooks`. | The hooks pin near-connect `^0.11` and pull in all of near-api-js; we need about 60 lines. |
| D47 | (6 Nov) The frontend calls the backend under **`/api`**; the Vite dev proxy (and later a Vercel rewrite) strips the prefix. Rejected: proxying each backend path. | Same-origin cookie (D37) with one rule, and no clash between frontend pages (`/avatars/…`) and backend paths. |
| D48 | (6 Nov) React owns the data (REST + `/ws`) and pushes it to Phaser through `townBus`, a `Phaser.Events.EventEmitter` (`town:ready`, `villages:updated`, `village:history`). | TECH §8 event bus with no new dependency; one owner for the data. |
| D49 | (6 Nov) The `/ws` client reconnects after a **fixed 5 s** when the socket closes. Rejected: no reconnect; backoff. | A backend restart must not freeze live labels until a reload (F8.5). |
| D50 | (6 Nov) Frontend tests: **vitest for pure helpers** only (`format.ts`, `parseLiveEvent`). Canvas and wallet are checked by hand. Rejected: no frontend tests. | Cheap and covers the parts with logic; a wallet popup cannot run in a unit test. |
| D51 | (6 Nov) The **backtest is optional** (SPEC F5 is now P2): no feature depends on it, and the track record is the live history. Supersedes D5, puts D35 on hold, and removes the backtest preview from the Day 11 create flow and the "backtest-only" leaderboard badge. The engine code stays; `POST /avatars` still runs the mock backtest but the UI does not show it. The demo seed characters start live **as early as possible** (as soon as `LLM_*` is set) so they have days of `sim` history by 14 Nov. Rejected: dropping the backtest code; stopping the create-time call now (code change for no visible gain); building D35 now. | Ayoub's call: an LLM backtest costs 30 calls per character and a slow create, and is weak evidence on 30 days of stablecoin APYs. The early live seed covers D5's original worry. |
| D52 | (6 Nov) Art = **Ninja Adventure asset pack** (Pixel-boy & AAA, CC0, downloaded from itch.io "Asset pack", not the Godot project). Only the used files are copied to `frontend/public/assets/`, credits in `CREDITS.md`. Rejected: Kenney Tiny Town + Tiny Dungeon (no walk frames), keeping the code-drawn shapes. | Real 4-direction walk cycles; free for any use. |
| D53 | (6 Nov) The ground is a **tile array built in code** (`game/townMap.ts`: grass, 2-tile 45° roads, 5×5 arena), not a Tiled map. Village positions stay the DB `map_x/map_y`, no migration. Rejected: the Tiled editor. | Diffable, testable, no hand-drawn file; our map is 4 roads and an arena. |
| D54 | (6 Nov) Sprites = a fixed list of 8 ids (`boy, princess, old_man, hunter, monk, noble, samurai, ninja_blue`), checked by the backend (`api::avatars::SPRITES`, 400 otherwise; replaces D42's character rule) and mirrored in `frontend/src/game/sprites.ts` (unknown ids drawn as `boy`). Rejected: frontend fallback only. | F3.1 "pick from 6 to 10"; no invented sprite names in the DB. The list lives in 2 places, each file points to the other. |
| D55 | (6 Nov) Demo seed = `scripts/seed_demo.sql`: 10 `sim` characters owned by `demo.fondouk.testnet`, fixed ids, `ON CONFLICT DO NOTHING` (safe to re-run). Personas drafted by Claude, **to be reviewed by Ayoub**. | D51: they must tick live for days before the demo. |
| D56 | (6 Nov) Sprite placement and walk: stand at `main_village` (queue on the road in front of the house, 3 abreast; all cash = grid in the arena); walk = straight to the arena centre, then to the new spot, 200 px/s (< 3 s, F8 "within 5 s"). Village "characters inside" = `main_village`. | No pathfinding needed: every road meets at the arena. One rule for the map and the panel. |

---

## 3. Unknowns to resolve first

Do these before writing product code. Each one can change the plan.

| # | Question | How to check | Result |
| --- | --- | --- | --- |
| U1 | DefiLlama yield history for Rhea pools: free? complete? | Call `/yields/pools`, filter for Rhea, then `/yields/chart/{pool}` | **Free, no key** (2 Nov). `https://yields.llama.fi/pools` lists 10 Rhea pools (`rhea-lend`, `rhea-dex`, `rhea-lst`). `https://yields.llama.fi/chart/{pool}` gives about 1 point per day (43 to 46 points since 18–21 Sep) plus a latest point refreshed during the day. Enough for a 30-day daily backtest. Values are **APY** (`apyBase`, `apyReward`). For lending pools, `tvlUsd` = supplied minus borrowed (available liquidity). Candidate villages: lend USDC `1a54c607-d829-5621-87a7-e3b7debc51a1` (TVL 1.6M, 14.0%), lend USDT `d4c96d26-5869-5771-90d5-08fd54e1455e` (1.6M, 13.1%), dex USDT-USDC-USDT.e-USDC.e `2fc37959-7f8b-5016-a18d-902ab72b0d4b` (104k, 8.1%), dex FRAX-USDC `94a8d511-c96e-5503-a81d-3357877d9c8a` (11.5k, 11.3%). Only 2 Rhea DEX pools are listed. **On-chain ids (6 Nov):** the stable pool is Rhea pool `4179` on `v2.ref-finance.near` (RATED_SWAP, fee 2 bps), FRAX-USDC is pool `4514` (RATED_SWAP, fee 2 bps). |
| U2 | Does Rhea have a testnet deployment? | https://guide.rhea.finance/ and https://github.com/rhea-finance | **Partly answered (5 Nov, web research only).** Old Burrow testnet contracts exist (SDK samples use `contract.1689937928.burrow.testnet`, test tokens `usdc.fakes.testnet` / `usdt.fakes.testnet`; a third-party repo mentions `contract.burrow.testnet` with a mock oracle). None of them is an official, maintained testnet. **To check on Ayoub's machine** (this cloud container blocks NEAR RPC): `get_config` / `get_asset` on those ids with near-cli. If none works, real mode = mainnet with tiny caps (SPEC Q2). |
| U3 | Rhea Lend contract id and supply/withdraw flow when called from another contract | Rhea docs and contract source | **Flow known, contract caller not proven (5 Nov).** Supply = `ft_transfer_call` on the **token** contract, `receiver_id` = `contract.main.burrow.near`, `msg: ""`, 1 yoctoNEAR. Plain supply earns interest (no `IncreaseCollateral` needed). Withdraw = `execute {"actions":[{"Withdraw":{"token_id":"<token>"}}]}` on Burrow with 1 yoctoNEAR (no amount = withdraw all); tokens come back to the caller with `ft_transfer`, limited by available liquidity. Before the first supply: `storage_deposit` on Burrow (README example 0.1 NEAR) and the vault must be registered on the token contract. No caller whitelist is documented, but the contract source was not read yet. Per-asset flags `can_deposit` / `can_withdraw` in `get_asset` config can block us. **Risk:** Rhea was exploited on 16 Apr 2026 (~$18.4M, margin trading slippage bug) and paused lending; a phased reopening from May was announced. Our indexer still saw moving APYs on 2 Nov, which suggests lending is live, but check `can_deposit` / `can_withdraw` on mainnet before Day 10. Sources: github.com/NearDeFi/burrowland (archived Jan 2023), github.com/burrowHQ/burrow-sdk-python, theblock.co post-mortem (17 Apr 2026). |
| U4 | Rhea view methods to compute live APR and TVL | Rhea docs and source, test with near-api view calls | **Lending: solved** (2 Nov). Rhea Lend = `contract.main.burrow.near` (mainnet). `get_asset {"token_id": "<token>"}` returns `supply_apr` as a decimal string (`0.138` = 13.8% **APR**), `supplied.balance`, `borrowed.balance` and `farms[].rewards` (currently empty, so reward APR = 0). Balances use token decimals + `config.extra_decimals` (USDC/USDT: 6 + 12 = 18). `get_assets_paged` gives raw state **without** APR. Native USDC token = `17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1`, USDT = `usdt.tether-token.near`. **DEX pools: not solved on-chain.** Fee APR needs 24h volume, which a view call does not give. Use DefiLlama for DEX villages. |
| U5 | Best model for strict JSON output | Test 2 to 3 models with the action schema: on OpenRouter first (dev, D17), then on NEAR AI Cloud (`https://cloud-api.near.ai/v1/model/list`) before the demo | **Still open (6 Nov).** First real run with `google/gemma-4-26b-a4b-it:free` on OpenRouter: every call got **HTTP 429** ("temporarily rate-limited upstream"), so JSON quality is not measured yet. The fallback worked as designed: retry once, then `HOLD` with `rejected_rule = llm_error`. Free models cannot serve 10 avatars × 144 ticks/day: pick a paid model or NEAR AI Cloud before running the seed. |
| U6 | async-openai works with the OpenRouter and NEAR AI Cloud base URLs | Small test program | **OpenRouter: yes** (6 Nov, live run): requests reach OpenRouter and the answer is read. One wart: OpenRouter's error body has a numeric `code`, so async-openai logs "failed to deserialize api response" instead of a clean error (the raw message is still in the log, the outcome is still HOLD). NEAR AI Cloud: not tested yet. |
| U7 | Latest stable near-sdk, cargo-near, near-sandbox, React, Vite, near-connect versions | `cargo add`, `npm view <pkg> version` | **Done** (2 Nov). Crates: axum 0.8.9, tokio 1.54.1, sqlx 0.9.0, near-api 0.8.7, async-openai 0.43.0 (replaces rig, D16), serde 1.0.229, serde_json 1.0.154, tracing 0.1.44, tracing-subscriber 0.3.23, dotenvy 0.15.7, reqwest 0.13.5, anyhow 1.0.104, thiserror 2.0.21, rust_decimal 1.43.0, chrono 0.4.45, near-sdk **5.30.0** (no 6.x yet), near-sandbox 0.4.0. Tools: cargo-near 0.22.0, near-cli-rs 0.30.1, sqlx-cli 0.9.0. npm: phaser 4.2.1, react 19.3.1, vite 8.3.5, typescript 7.0.2, @hot-labs/near-connect 0.12.0, near-connect-hooks 1.1.6. Note: near-sandbox has no native Windows build, so run sandbox tests in WSL. |

Fill the "Result" column and commit.

---

## 4. Day-by-day plan

Times are rough. P0 first, always.

### Before 1 Nov (prep, no product code)
- Resolve U1 to U7 with throwaway scripts.
- Start recording Rhea APR snapshots every hour into a local file (our own history fallback).
- Pick a free tileset and 6 to 10 character sprites. Check licenses.
- Create the GitHub repo (private for now), add these docs.

### Week 1
| Day | Date | Goal | Done when |
| --- | --- | --- | --- |
| 1 | 1 Nov | Repo skeleton, Postgres + migrations, axum hello, indexer for 4 villages | Yield rows stored every hour (D19). **Done 4 Nov** |
| 2 | 2 Nov | History import + backtest engine (with mock decide) | 30 equity rows for a test avatar. **Done 4 Nov** |
| 3 | 3 Nov | Agent loop: prompt, LLM call, schema parse, guardrails. **Vault spike** (Rhea Lend call flow) | 3 ticks logged for 5 avatars. Spike result written in this file. **Done 5 Nov** |
| 4 | 4 Nov | Sim ledger + cost model + tests | Cost and accrual tests pass. **Done 6 Nov** |
| 5 | 5 Nov | REST + WebSocket API, wallet auth | Endpoints return real data. **Done 6 Nov** |
| 6 | 6 Nov | Frontend shell: React + wallet + Phaser map loads villages | Map with live APR labels. **Done 6 Nov** (real wallet login works) |
| 7 | 7 Nov | Sprites, movement on decisions, side panels | Avatar walks after a decision. **Built 6 Nov**, walk not seen yet (LLM rate-limited, U5) |

### Week 2
| Day | Date | Goal | Done when |
| --- | --- | --- | --- |
| 8 | 8 Nov | Marketplace contract + tests, deploy testnet | Sandbox tests pass, testnet deployed |
| 9 | 9 Nov | Marketplace UI + event watcher + clone creation. **Real mode go/no-go** | One testnet clone works end to end |
| 10 | 10 Nov | Vault contract (full or fallback version) + executor | One real deposit, action, withdraw |
| 11 | 11 Nov | Leaderboard, create-avatar flow (no backtest preview, D51) | Leaderboard numbers match manual check |
| 12 | 12 Nov | Social arena (P1), demo seed check (10 avatars, **started live earlier**, D51), judge tour | Fresh visitor can do the tour |
| 13 | 13 Nov | Bug fixing, README for humans and AI judges, deploy | Public URL works from another machine |
| 14 | 14 Nov | Record demo video, X post, make repo public, submit before 23:59 UTC | Submission returns 201 |

**Cut order if late** (cut from the top): social arena listening (P2), social arena messages (P1), real mode with Rhea (keep vault deposit and withdraw), village count down to 3, custom sprites.

---

## 5. How to run

```bash
# prerequisites: Rust stable, sqlx-cli, PostgreSQL (later also: cargo-near, Node LTS, a NEAR testnet account)

# backend (works today)
cd backend
cp .env.example .env        # set DATABASE_URL; never commit .env
sqlx database create
sqlx migrate run            # creates villages + yield_snapshots, seeds the 4 villages
cargo run                   # API on 127.0.0.1:8080; the indexer fetches all villages at startup
curl http://127.0.0.1:8080/villages
cargo test                  # the DB user needs CREATEDB: ALTER USER fondouk CREATEDB;

# build without a database (uses the committed .sqlx/ cache, D26)
SQLX_OFFLINE=true cargo build
# after changing any query! SQL, refresh the cache (needs the DB) and commit .sqlx/
cargo sqlx prepare -- --all-targets
cargo sqlx prepare --check -- --all-targets   # fails if .sqlx/ is out of date

# frontend (works today; needs Node 22+ and the backend running on :8080)
cd frontend
npm install
npm run dev                 # http://localhost:5173, /api/* and /api/ws proxied to the backend (D47)
npm test                    # vitest
npm run lint                # oxlint
npm run build               # tsc type check + production build

# demo seed (D55): 10 sim characters; safe to re-run. Only once LLM_* points
# to a model that can serve ~1,440 calls a day (U5), or they only HOLD.
psql "$DATABASE_URL" -f scripts/seed_demo.sql

# run the backend with the agent loop off (no LLM calls), e.g. to look at the UI
LLM_BASE_URL= LLM_API_KEY= LLM_MODEL= cargo run

# not built yet:

# contracts
cd contracts/marketplace
cargo near build
cargo test
```

Environment variables. The first five, the `LLM_*` / `AGENT_TICK_SECS` ones and `NEAR_RPC_URL` are used today (see `backend/.env.example`); the rest are planned.

| Name | Purpose |
| --- | --- |
| `DATABASE_URL` | Postgres connection (required) |
| `BIND_ADDR` | HTTP listen address, default `127.0.0.1:8080` |
| `RUST_LOG` | Log level, e.g. `info,backend=debug` |
| `INDEXER_INTERVAL_SECS` | Seconds between indexer rounds, default 3600, minimum 60 (D19) |
| `VILLAGE_STALE_AFTER_SECS` | Stale threshold, default 2 x the interval, must be longer than it (D20) |
| `AGENT_TICK_SECS` | Seconds between agent ticks, default 600, minimum 60 |
| `NEAR_NETWORK` | `testnet` or `mainnet` |
| `NEAR_RPC_URL` | RPC endpoint. Used today by login (key check, D38). Default `https://test.rpc.fastnear.com` (`rpc.testnet.near.org` is deprecated) |
| `OPERATOR_ACCOUNT_ID` | Account that owns the vault operator function-call key |
| `OPERATOR_PRIVATE_KEY` | Function-call key only. Never a full-access key. |
| `MARKETPLACE_CONTRACT_ID` | Deployed marketplace |
| `VAULT_CONTRACT_ID` | Deployed vault |
| `LLM_API_KEY` | Key for the provider at `LLM_BASE_URL` (OpenRouter in dev, NEAR AI Cloud for the demo). `LLM_*`: set all three or none; none = agent loop off. |
| `LLM_MODEL` | Model id for that provider (e.g. `openai/gpt-4o-mini` on OpenRouter) |
| `LLM_BASE_URL` | Dev: `https://openrouter.ai/api/v1`. Demo: `https://cloud-api.near.ai/v1` (D17) |
| `VITE_NEAR_NETWORK` | Frontend (`frontend/.env.local`): wallet network, `testnet` (default) or `mainnet`. Must match `NEAR_RPC_URL`. Public, in the browser bundle. |
| `VITE_WS_URL` | Frontend: live events URL. Empty = `/api/ws` through the dev proxy. In production the backend's `wss://…/ws` (Vercel does not proxy WebSockets, D37). |

---

## 6. Working rules

- **Never commit secrets.** `.env` is in `.gitignore`. Check before every push.
- **Never deploy to mainnet or sign mainnet transactions** without Ayoub's explicit OK.
- Every LLM decision is logged with input hash, action, valid flag, rule that rejected it.
- Backtest, sim and real data are always labeled in the database and the UI.
- Small PRs, one feature each. Main branch must always run.
- Update section 1, 3 and 7 of this file at the end of every session.

---

## 7. Session log

| Date | Who | What changed | Next step |
| --- | --- | --- | --- |
| 23 Oct 2026 | Ayoub + Claude | Idea, name, docs (overview, SPEC, TECH_architecture, HANDOFF) | Resolve unknowns U1 to U7 |
| 2 Nov 2026 | Ayoub + Claude | Tools installed. U1, U4, U7 resolved. Decisions D13 to D20 (DefiLlama source, villages, async-openai, LLM provider by env, DB design in `dev-docs/2026-11-02/database-design.md`, fetch interval, stale threshold). Backend skeleton, villages + yield_snapshots storage, DefiLlama indexer. | Finish Day 1 (`GET /villages`) |
| 3 Nov 2026 | Ayoub | AI working rules copied into the repo (`docs/AI_WORKING_RULES.md`) | Same |
| 4 Nov 2026 | Ayoub + Claude | `GET /villages` with stale flag. All 38 tests pass (CREATEDB granted). Docs catch-up (stale rule wording, how to run). Day 1 done. | Day 2 plan (history import + backtest); review the cut order (2 days behind) |
| 4 Nov 2026 | Ayoub + Claude | Day 2 done in 5 phases: full history import (D22), users / avatar_configs / avatars and equity_snapshots tables, daily history query (UTC days), paper ledger with exact APY compounding (D21, D23), SPEC action enum, mock decide, backtest engine (D24). 99 tests pass. SPEC F6.2 and TECH 5.3 now use the APY formula. | Day 3: agent loop + vault spike. Open: how actions treat cash (MOVE from cash?) |
| 4 Nov 2026 | Ayoub + Claude | Day 2 close-out: kept `CHECK (equity_usd >= 0)` and documented it (D25); committed the `.sqlx/` query cache for offline builds (D26). | Day 3: agent loop + vault spike. Open: how actions treat cash |
| 5 Nov 2026 | Ayoub + Claude | Day 3: guardrails (D27, D29) and MOVE in the backtest; LLM decide pipeline (async-openai 0.43.0, prompt, parse, retry once, HOLD, `llm_cache`); migration 0005 (`decisions`, `llm_cache`); agent tick loop that logs decisions (D30); cost rules moved to Day 4 (D28). Vault spike: U2 partly, U3 flow known (see section 3), Rhea April exploit noted. 151 tests pass. | Day 4: positions + cost model. Ayoub: add `LLM_*` to test a real model (U5, U6); run the U2/U3 near-cli checks. |
| 6 Nov 2026 | Ayoub + Claude | Day 4: migration 0006 (`positions`, `villages.fee_bps` + `tokens`, fees checked on-chain); kind-aware cost model `sim::costs` (D31); cost guardrails `min_gain`, `cost_too_high` and `no_swap_route` (D28, D32); ledger pays costs and accrues any duration (F6.2); live tick moves paper money, accrues real elapsed time (D33) and stores positions + cash + decision + sim equity in one transaction (D34); backtest pays costs; F6 acceptance test (frequent mover ends below holder). 189 tests pass. | Backtest on the LLM (D35), then Day 5 API. Ayoub: `LLM_*` key (U5, U6), U2/U3 checks. |
| 6 Nov 2026 | Ayoub + Claude | Day 5 (before D35, D36): migration 0007 (`auth_challenges`, `sessions`); NEP-413 login + cookie sessions (D37, D38); avatar REST endpoints with persona privacy, create + backtest, owner-only PATCH (D39–D42); village history; `/ws` live events via one broadcast channel (D43). New deps: ed25519-dalek, bs58, base64, getrandom, axum `ws`, uuid `serde`; dev: tokio-tungstenite, futures-util. 253 tests pass. | D35 (backtest on the LLM), then Day 6 frontend: first real NEAR Connect login, Vite proxy for same-origin cookies. |
| 6 Nov 2026 | Ayoub + Claude | Day 6 before D35 (D44): `frontend/` with Vite 8 + React 19 + TypeScript 7, Phaser 4.2.1, `@hot-labs/near-connect` 0.12.0 (D46), vitest (D50), oxlint (template default). Placeholder town drawn in code with live APY labels, 7-day sparklines and stale badges (D45); `/api` proxy (D47); `townBus` (D48); `/ws` client with 5 s reconnect (D49); NEP-413 login button. 13 frontend tests, backend still 253. No backend change. | Ayoub: run backend + `npm run dev`, try a real testnet wallet login. Then D35, then Day 7. |
| 6 Nov 2026 | Ayoub + Claude | Day 6 committed (`ed340d7`). D35 plan discussed (LLM backtest), then Ayoub made the backtest **optional** (D51): SPEC F5 → P2, G3 / F3.3 / F10.1 / F12.1 updated, TECH §5.2 and risks updated, D5 superseded, D35 on hold. Docs only, no code change. | Day 7 (sprites, movement, side panels). Ayoub: add `LLM_*` (U5, U6), then start the demo seed live early (D51). |
| 6 Nov 2026 | Ayoub + Claude | Day 7 built (D52–D56): Ninja Adventure art (CC0), tile ground in code, 8 sprites (backend whitelist), queue/arena placement and walk through the arena centre, avatar and village side panels with SVG charts, live panel refresh, `scripts/seed_demo.sql`. First real LLM run: OpenRouter reached (U6) but free gemma answered 429, so HOLD `llm_error` (U5 open). Seed avatar #1 (Amira) is in the dev DB with 4 such HOLDs. Checked in Chrome. 36 frontend, 254 backend tests. | Ayoub: pick the seed's model/provider and review the personas, then run the seed and watch a sprite walk. Then Day 8. |

---

## 8. Submission checklist (14 Nov)

Source: https://nearhackathon.com/HACKATHON.md. Re-read it on 13 Nov, rules can change.

- [ ] Repo is **public**.
- [ ] README explains: what it is, the problem, how to try it as "do X, see Y" steps, which NEAR features are used (function-call keys, NEAR AI Cloud, Rhea, NEP-141, NEP-297), and links to deployed contracts and example transactions. Written for AI judges too, no marketing.
- [ ] Live demo URL works without a wallet (judge tour).
- [ ] Demo video recorded (2 to 3 minutes).
- [ ] X post published by Ayoub: says it was built at NEAR HACKATHON, video or screenshots attached.
- [ ] Mainnet NEAR account for prize payout created by Ayoub.
- [ ] Description written (20 to 2000 characters): problem, product, how NEAR fits.
- [ ] Tracks chosen (ids: `games`, `agents`, `security`, `privacy`, maybe `defi`, `swarms`).
- [ ] Submission sent **once**, before 14 Nov 2026 23:59 UTC, via https://nearhackathon.com/#apply or the API. Expect HTTP 201. Do not fill the `website` field (it is a spam trap).

---

## 9. Links

| What | Link |
| --- | --- |
| Hackathon site | https://nearhackathon.com |
| Hackathon agent file | https://nearhackathon.com/HACKATHON.md |
| Submission | https://nearhackathon.com/#apply |
| NEAR docs | https://docs.near.org/ |
| NEAR AI Cloud docs | https://docs.near.ai/ |
| NEAR AI Cloud API base | https://cloud-api.near.ai/v1 |
| NEAR Intents docs | https://docs.near-intents.org/ |
| Rhea docs | https://guide.rhea.finance/ |
| Rhea GitHub | https://github.com/rhea-finance |
| Rhea app | https://rhea.finance/ |
| OutLayer (secure tx for agents, partner, worth a look for the vault) | https://app.outlayer.ai/docs/getting-started |
| NEAR Discord | https://discord.gg/nearprotocol |
| NEAR on X | https://x.com/NEARProtocol |
