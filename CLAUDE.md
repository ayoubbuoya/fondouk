# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project state

Fondouk: an AI town on NEAR where each character is an LLM-driven DeFi agent that moves funds between real yield "villages" (Rhea pools), with an on-chain clone marketplace. Built for NEAR HACKATHON (1–14 Nov 2026, deadline 14 Nov 23:59 UTC).

As of 23 Oct 2026 the repo contains **only planning docs** in `docs/`. Product code starts on 1 Nov 2026 (hackathon rule: before that, only docs, research and throwaway spikes, nothing that becomes product code).

Read in this order: `docs/HANDOFF.md` (current state, decisions D1–D12, unknowns U1–U7, day plan) → `docs/SPEC.md` (features F1–F12, action set, guardrails) → `docs/TECH_architecture.md` (modules, schema, contracts). When code lands, update this file with real commands.

## Planned commands (from HANDOFF §5, not yet verified)

```bash
# backend (Rust, axum + sqlx + Postgres)
cd backend && cp .env.example .env && sqlx migrate run && cargo run
# frontend (React + Vite + Phaser 4)
cd frontend && npm install && npm run dev
# contracts (near-sdk-rs)
cd contracts/marketplace && cargo near build && cargo test
```

Planned layout: `backend/` (one modular-monolith binary: indexer, backtest, sim, agent, executor, events, api), `contracts/marketplace`, `contracts/vault`, `frontend/` (`game/` Phaser, `ui/` React, connected by a small event bus), `scripts/`. Env vars are listed in HANDOFF §5.

Package versions in TECH_architecture §3 are often marked "verify on setup". Use whatever `cargo add` / `npm install` resolves to, not the doc's version numbers.

## Cross-cutting invariants

- **The LLM proposes, code decides.** The LLM may only return one of `HOLD | MOVE | REBALANCE | SAY` (SPEC §5). Parse failure → retry once → `HOLD`. Every action goes through the risk-level guardrails table (SPEC §6) in Rust. A violation becomes `HOLD`, and the rejecting rule is logged in `decisions.rejected_rule`. Guardrails never live only in the prompt.
- **One `decide()` for everything.** `decide(market_state, config) -> action` is a pure boundary shared by the live tick loop and the 30-day backtest. Don't fork the logic between them.
- **The persona is private.** It is returned only to the owner. It never appears in WebSocket messages, arena messages (tested: no 8-word overlap with the persona) or clones. Clones point to the source's `avatar_configs.config_id` and never copy the text. Clones cannot be re-listed.
- **Data source labels.** `backtest`, `sim` and `real` results are stored separately (`equity_snapshots.source`, `decisions.source`) and are never merged into one number in the UI.
- **Vault security model.** The backend holds only a *function-call access key* restricted to the vault's non-payable `agent_supply` / `agent_withdraw`. Function-call keys cannot attach deposits, and NEP-141 `ft_transfer_call` needs 1 yoctoNEAR, so the *vault* attaches it when calling Rhea Lend. Caps, whitelist and cooldown are enforced on-chain as well as in the backend. Callbacks are `#[private]` and must handle failure. Only the owner can `withdraw`.
- **Marketplace split.** 90% to the creator, 10% to the platform (`platform_fee_bps`). Any rounding remainder goes to the creator. Emits a NEP-297 `clone_purchased` event. The backend watcher is idempotent by tx hash.
- Stale villages (snapshot older than 30 min) cannot receive funds.

## Working rules (HANDOFF §6)

- Never commit secrets. `OPERATOR_PRIVATE_KEY` must be a function-call key, never a full-access key.
- Never deploy to mainnet or sign mainnet transactions without Ayoub's explicit OK.
- At the end of each work session, update HANDOFF §1 (state), §3 (unknowns) and §7 (session log).
- If real mode with Rhea isn't working by day 9 (9 Nov), fall back to deposit/hold/withdraw only (SPEC F7.5). The cut order is in HANDOFF §4.
