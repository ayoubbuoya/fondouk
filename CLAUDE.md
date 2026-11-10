# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Working rules for the AI

The approval gate, plan-then-explain workflow, simplicity-first, and end-of-task format all live in a separate file. They apply to every session in this repo:

@docs/AI_WORKING_RULES.md

## Project state

Fondouk: an AI town on NEAR where each character is an LLM-driven DeFi agent that moves funds between real yield "villages" (Rhea pools), with an on-chain clone marketplace. Built for NEAR HACKATHON (1–14 Nov 2026, deadline 14 Nov 23:59 UTC).

As of 6 Nov 2026: `backend/` (axum + sqlx + Postgres) has the DefiLlama indexer for 4 villages, paper ledger + cost model, backtest engine (runs the live LLM `decide` with the persona, shown in the create wizard, D63), guardrails, LLM decide pipeline, live agent tick loop, REST API, NEP-413 wallet login and `/ws` live events (Days 1–5). `frontend/` (Vite + React + TypeScript + three.js + NEAR Connect) shows the 3D medina town (D76, D77) with live APY labels, walking 3D characters, avatar and village side panels, a login button, the Marketplace panel and the create-avatar wizard (Days 6–9, D63). `scripts/seed_demo.sql` seeds the 10 demo characters. No contracts yet. The DB design is in `dev-docs/2026-11-02/database-design.md`.

Read in this order: `docs/HANDOFF.md` (current state, decisions D1–D63, unknowns U1–U7, day plan) → `docs/SPEC.md` (features F1–F12, action set, guardrails) → `docs/TECH_architecture.md` (modules, schema, contracts).

## Commands

```bash
# backend (works today; DATABASE_URL in backend/.env, the DB user needs CREATEDB for tests)
cd backend && sqlx migrate run && cargo run      # API on 127.0.0.1:8080
cd backend && cargo test                         # all tests, incl. #[sqlx::test] with temp DBs
cd backend && cargo clippy --all-targets -- -D warnings
# frontend (works today; the backend must run on :8080, /api is proxied to it)
cd frontend && npm install && npm run dev        # http://localhost:5173
cd frontend && npm test && npm run lint && npm run build
# planned, not built yet:
# contracts (near-sdk-rs)
cd contracts/marketplace && cargo near build && cargo test
```

Planned layout: `backend/` (one modular-monolith binary: indexer, backtest, sim, agent, executor, events, api), `contracts/marketplace`, `contracts/vault`, `frontend/` (`game/` layout + `game3d/` three.js town, `ui/` React, connected by a small event bus), `scripts/`. Env vars are listed in HANDOFF §5.

Package versions in TECH_architecture §3 are often marked "verify on setup". Use whatever `cargo add` / `npm install` resolves to, not the doc's version numbers.

## Cross-cutting invariants

- **The LLM proposes, code decides.** The LLM may only return one of `HOLD | MOVE | REBALANCE | SAY` (SPEC §5). Parse failure → retry once → `HOLD`. Every action goes through the risk-level guardrails table (SPEC §6) in Rust. A violation becomes `HOLD`, and the rejecting rule is logged in `decisions.rejected_rule`. Guardrails never live only in the prompt.
- **One `decide()` for everything.** `decide(market_state, config) -> action` is a pure boundary shared by the live tick loop and the 7-day backtest, if the backtest is used (it is optional, D51). Don't fork the logic between them.
- **The persona is private.** It is returned only to the owner. It never appears in WebSocket messages, arena messages (tested: no 8-word overlap with the persona) or clones. Clones point to the source's `avatar_configs.config_id` and never copy the text. Clones cannot be re-listed.
- **Data source labels.** `backtest`, `sim` and `real` results are stored separately (`equity_snapshots.source`, `decisions.source`) and are never merged into one number in the UI.
- **Vault security model.** The backend holds only a *function-call access key* restricted to the vault's non-payable `agent_supply` / `agent_withdraw`. Function-call keys cannot attach deposits, and NEP-141 `ft_transfer_call` needs 1 yoctoNEAR, so the *vault* attaches it when calling Rhea Lend. Caps, whitelist and cooldown are enforced on-chain as well as in the backend. Callbacks are `#[private]` and must handle failure. Only the owner can `withdraw`.
- **Marketplace split.** 90% to the creator, 10% to the platform (`platform_fee_bps`). Any rounding remainder goes to the creator. Emits a NEP-297 `clone_purchased` event. The backend watcher is idempotent by tx hash.
- Stale villages cannot receive funds. Stale = no successful indexer fetch for longer than `VILLAGE_STALE_AFTER_SECS` (default 2 x the 1-hour fetch interval, HANDOFF D19/D20). One rule, `indexer::is_stale`, used by the API and the agents.

## Working rules (HANDOFF §6)

- Never commit secrets. `OPERATOR_PRIVATE_KEY` must be a function-call key, never a full-access key.
- Never deploy to mainnet or sign mainnet transactions without Ayoub's explicit OK.
- At the end of each work session, update HANDOFF §1 (state), §3 (unknowns) and §7 (session log). Approved decisions also go into HANDOFF §2 (`D<n>`), because teammates cannot read Claude's local memory.
- If real mode with Rhea isn't working by day 9 (9 Nov), fall back to deposit/hold/withdraw only (SPEC F7.5). The cut order is in HANDOFF §4.

## Session continuity (Claude memory)

Write to the project memory folder **when something happens**, not at session end. Sessions can stop at any time.

- **Decision approved** → save a `project` memory: the decision, the date, why, and the options that were rejected (so they are not proposed again). Also add it to HANDOFF §2.
- **Plan approved** → save a `project` memory with the plan. When the work is finished, update that same memory: mark it done, add the commit hash if one exists, and note anything that differed from the plan.
- **Waiting on Ayoub** → save a `project` memory with the open question and the options presented. Delete it once Ayoub answers.
- **How Ayoub wants me to work** (corrections, preferences) → save a `feedback` memory.
- Update an existing memory instead of creating a duplicate. Delete memories that are no longer true.

At the start of every session, read the memory index and the relevant memories **before** proposing anything. Continue from the active plan. Do not re-open a decision that is saved in memory or listed in HANDOFF §2 unless Ayoub asks.

Saving or updating memories needs no separate approval, but mention it in "Files changed".
