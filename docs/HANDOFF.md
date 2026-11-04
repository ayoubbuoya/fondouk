# Fondouk: Handoff

Last updated: 4 Nov 2026. Update this file at the end of every work session.

This file is for anyone joining the project (teammates or a coding agent). Read it first, then `overview.md`, `SPEC.md`, `TECH_architecture.md`.

---

## 1. Current state

- **Phase:** Day 1 backend done on 4 Nov (about 2 days behind the day plan in section 4). Next: Day 2, history import + backtest engine.
- **What runs today:** `backend/` (axum + sqlx + Postgres). DefiLlama indexer for the 4 villages (D13, D14), fetching every hour (D19). `GET /health`, `GET /villages` (latest yield + stale flag). 38 tests pass. No frontend or contracts yet.
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
| D5 | 30-day backtest on character creation. | Without it, there is no 7d or 30d history at demo time (the hackathon is 14 days). |
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

---

## 3. Unknowns to resolve first

Do these before writing product code. Each one can change the plan.

| # | Question | How to check | Result |
| --- | --- | --- | --- |
| U1 | DefiLlama yield history for Rhea pools: free? complete? | Call `/yields/pools`, filter for Rhea, then `/yields/chart/{pool}` | **Free, no key** (2 Nov). `https://yields.llama.fi/pools` lists 10 Rhea pools (`rhea-lend`, `rhea-dex`, `rhea-lst`). `https://yields.llama.fi/chart/{pool}` gives about 1 point per day (43 to 46 points since 18–21 Sep) plus a latest point refreshed during the day. Enough for a 30-day daily backtest. Values are **APY** (`apyBase`, `apyReward`). For lending pools, `tvlUsd` = supplied minus borrowed (available liquidity). Candidate villages: lend USDC `1a54c607-d829-5621-87a7-e3b7debc51a1` (TVL 1.6M, 14.0%), lend USDT `d4c96d26-5869-5771-90d5-08fd54e1455e` (1.6M, 13.1%), dex USDT-USDC-USDT.e-USDC.e `2fc37959-7f8b-5016-a18d-902ab72b0d4b` (104k, 8.1%), dex FRAX-USDC `94a8d511-c96e-5503-a81d-3357877d9c8a` (11.5k, 11.3%). Only 2 Rhea DEX pools are listed. |
| U2 | Does Rhea have a testnet deployment? | https://guide.rhea.finance/ and https://github.com/rhea-finance | _todo_ |
| U3 | Rhea Lend contract id and supply/withdraw flow when called from another contract | Rhea docs and contract source | _todo_ |
| U4 | Rhea view methods to compute live APR and TVL | Rhea docs and source, test with near-api view calls | **Lending: solved** (2 Nov). Rhea Lend = `contract.main.burrow.near` (mainnet). `get_asset {"token_id": "<token>"}` returns `supply_apr` as a decimal string (`0.138` = 13.8% **APR**), `supplied.balance`, `borrowed.balance` and `farms[].rewards` (currently empty, so reward APR = 0). Balances use token decimals + `config.extra_decimals` (USDC/USDT: 6 + 12 = 18). `get_assets_paged` gives raw state **without** APR. Native USDC token = `17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1`, USDT = `usdt.tether-token.near`. **DEX pools: not solved on-chain.** Fee APR needs 24h volume, which a view call does not give. Use DefiLlama for DEX villages. |
| U5 | Best model for strict JSON output | Test 2 to 3 models with the action schema: on OpenRouter first (dev, D17), then on NEAR AI Cloud (`https://cloud-api.near.ai/v1/model/list`) before the demo | _todo_ (needed for Day 3) |
| U6 | async-openai works with the OpenRouter and NEAR AI Cloud base URLs | Small test program | _todo_ (needed for Day 3). Docs confirm a custom base URL is supported; not tested yet. |
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
| 2 | 2 Nov | History import + backtest engine (with mock decide) | 30 equity rows for a test avatar |
| 3 | 3 Nov | Agent loop: prompt, LLM call, schema parse, guardrails. **Vault spike** (Rhea Lend call flow) | 3 ticks logged for 5 avatars. Spike result written in this file |
| 4 | 4 Nov | Sim ledger + cost model + tests | Cost and accrual tests pass |
| 5 | 5 Nov | REST + WebSocket API, wallet auth | Endpoints return real data |
| 6 | 6 Nov | Frontend shell: React + wallet + Phaser map loads villages | Map with live APR labels |
| 7 | 7 Nov | Sprites, movement on decisions, side panels | Avatar walks after a decision |

### Week 2
| Day | Date | Goal | Done when |
| --- | --- | --- | --- |
| 8 | 8 Nov | Marketplace contract + tests, deploy testnet | Sandbox tests pass, testnet deployed |
| 9 | 9 Nov | Marketplace UI + event watcher + clone creation. **Real mode go/no-go** | One testnet clone works end to end |
| 10 | 10 Nov | Vault contract (full or fallback version) + executor | One real deposit, action, withdraw |
| 11 | 11 Nov | Leaderboard, create-avatar flow with backtest preview | Leaderboard numbers match manual check |
| 12 | 12 Nov | Social arena (P1), demo seed (10 avatars), judge tour | Fresh visitor can do the tour |
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

# not built yet:

# frontend
cd frontend
npm install
npm run dev

# contracts
cd contracts/marketplace
cargo near build
cargo test
```

Environment variables. The first five are used today (see `backend/.env.example`); the rest are planned.

| Name | Purpose |
| --- | --- |
| `DATABASE_URL` | Postgres connection (required) |
| `BIND_ADDR` | HTTP listen address, default `127.0.0.1:8080` |
| `RUST_LOG` | Log level, e.g. `info,backend=debug` |
| `INDEXER_INTERVAL_SECS` | Seconds between indexer rounds, default 3600, minimum 60 (D19) |
| `VILLAGE_STALE_AFTER_SECS` | Stale threshold, default 2 x the interval, must be longer than it (D20) |
| `NEAR_NETWORK` | `testnet` or `mainnet` |
| `NEAR_RPC_URL` | RPC endpoint |
| `OPERATOR_ACCOUNT_ID` | Account that owns the vault operator function-call key |
| `OPERATOR_PRIVATE_KEY` | Function-call key only. Never a full-access key. |
| `MARKETPLACE_CONTRACT_ID` | Deployed marketplace |
| `VAULT_CONTRACT_ID` | Deployed vault |
| `LLM_API_KEY` | Key for the provider at `LLM_BASE_URL` (OpenRouter in dev, NEAR AI Cloud for the demo) |
| `LLM_MODEL` | Model id for that provider (e.g. `openai/gpt-4o-mini` on OpenRouter) |
| `LLM_BASE_URL` | Dev: `https://openrouter.ai/api/v1`. Demo: `https://cloud-api.near.ai/v1` (D17) |

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
