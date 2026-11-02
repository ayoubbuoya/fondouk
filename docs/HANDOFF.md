# Fondouk: Handoff

Last updated: 23 Oct 2026. Update this file at the end of every work session.

This file is for anyone joining the project (teammates or a coding agent). Read it first, then `overview.md`, `SPEC.md`, `TECH_architecture.md`.

---

## 1. Current state

- **Phase:** planning done, no product code yet.
- **Product code starts:** 1 Nov 2026, 00:00 UTC (hackathon start).
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
| D10 | Stack: Rust (axum, sqlx, near-api, rig), near-sdk-rs contracts, React + Vite + Phaser 4 frontend, NEAR AI Cloud for inference. | Team strength in Rust and Postgres. Phaser saves time on the map. NEAR AI Cloud fits the `privacy` track. |
| D11 | Modular monolith backend. | One binary is faster to build and debug solo. |
| D12 | Shade Agent Framework is **not** used. | NEAR docs say it is no longer maintained as standalone tooling after 19 Apr 2026. |

---

## 3. Unknowns to resolve first

Do these before writing product code. Each one can change the plan.

| # | Question | How to check | Result |
| --- | --- | --- | --- |
| U1 | DefiLlama yield history for Rhea pools: free? complete? | Call `/yields/pools`, filter for Rhea, then `/yields/chart/{pool}` | _todo_ |
| U2 | Does Rhea have a testnet deployment? | https://guide.rhea.finance/ and https://github.com/rhea-finance | _todo_ |
| U3 | Rhea Lend contract id and supply/withdraw flow when called from another contract | Rhea docs and contract source | _todo_ |
| U4 | Rhea view methods to compute live APR and TVL | Rhea docs and source, test with near-api view calls | _todo_ |
| U5 | Best NEAR AI Cloud model for strict JSON output | Test 2 to 3 models from `https://cloud-api.near.ai/v1/model/list` with the action schema | _todo_ |
| U6 | rig works with NEAR AI Cloud base URL | Small test program | _todo_ |
| U7 | Latest stable near-sdk, cargo-near, near-sandbox, React, Vite, near-connect versions | `cargo add`, `npm view <pkg> version` | _todo_ |

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
| 1 | 1 Nov | Repo skeleton, Postgres + migrations, axum hello, indexer for 4 villages | APR rows appear every 5 min |
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

## 5. How to run (to be filled on day 1)

```bash
# prerequisites: Rust stable, cargo-near, Node LTS, PostgreSQL, a NEAR testnet account

# backend
cd backend
cp .env.example .env        # fill values, never commit .env
sqlx migrate run
cargo run

# frontend
cd frontend
npm install
npm run dev

# contracts
cd contracts/marketplace
cargo near build
cargo test
```

Environment variables (planned):

| Name | Purpose |
| --- | --- |
| `DATABASE_URL` | Postgres connection |
| `NEAR_NETWORK` | `testnet` or `mainnet` |
| `NEAR_RPC_URL` | RPC endpoint |
| `OPERATOR_ACCOUNT_ID` | Account that owns the vault operator function-call key |
| `OPERATOR_PRIVATE_KEY` | Function-call key only. Never a full-access key. |
| `MARKETPLACE_CONTRACT_ID` | Deployed marketplace |
| `VAULT_CONTRACT_ID` | Deployed vault |
| `NEAR_AI_API_KEY` | NEAR AI Cloud key |
| `NEAR_AI_MODEL` | Chosen model id |
| `LLM_BASE_URL` | Default `https://cloud-api.near.ai/v1` |
| `DEFILLAMA_API_KEY` | Only if U1 shows the Pro key is needed |

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
