# Fondouk: Product Specification

Version 0.1, 23 Oct 2026. Target: NEAR HACKATHON (1 to 14 Nov 2026). See `overview.md` for context.

---

## 1. Goals and non-goals

### Goals
- G1. A user can create an AI DeFi character in under 2 minutes and see it act in a visual town.
- G2. Characters make decisions from **real, live on-chain APR data** from NEAR protocols.
- G3. Every character has a visible, honest track record (its live history), with risk metrics, not only returns. A backtest is an optional extra (F5, D51), not part of the track record.
- G4. Users can clone a character by paying its creator **on-chain**, without seeing the strategy text.
- G5. In real mode, the AI can never send funds out of the vault. Only the owner can withdraw.
- G6. The demo is understandable in 2 minutes by someone who knows nothing about DeFi.

### Non-goals (for the hackathon)
- No cross-chain execution (planned as next step via NEAR Intents).
- No leverage, margin or borrowing by agents.
- No profit-share payments (one-time clone fee only).
- No mobile app. Desktop browser first, must not break on mobile.
- No token launch.

---

## 2. Users

| Persona | Wants | Main features used |
| --- | --- | --- |
| Curious beginner | Earn yield without learning DeFi, have fun watching | Create character, simulation mode, town view |
| Strategy creator | Show skill, earn from good strategies | Character creation, leaderboard, marketplace (seller) |
| Copier | Use a proven strategy | Leaderboard, marketplace (buyer), real mode |
| Judge | Understand and try the product fast | Demo mode, README steps, seeded town |

---

## 3. Core concepts

| Concept | Definition |
| --- | --- |
| **Town** | The 2D map. Contains villages and the arena. |
| **Village** | One yield source. Has a kind (lending, LP, staking), an asset, a live APR, a TVL, and a position on the map. |
| **Character (avatar)** | An AI agent owned by a user. Has a persona, a risk profile, rules, a mode (sim or real), a balance and positions. |
| **Persona** | The private instructions written by the creator. Never exposed by any API to anyone except the owner. |
| **Tick** | One decision round. Every character receives the market state and returns one action. Default: every 10 minutes in live mode. |
| **Action** | One item from a fixed list (section 5). Free text actions are not allowed. |
| **Track record** | Live equity history (`sim` or `real`). A backtest, if one exists, is shown apart and marked as backtest (D51). |
| **Clone** | A new character for the buyer that uses the same private config as the source, by reference. |

---

## 4. Features and requirements

Priority: **P0** = must ship for demo. **P1** = should ship. **P2** = only if time allows.

### F1. Wallet login (P0)
- F1.1 User connects a NEAR wallet through NEAR Connect.
- F1.2 Backend links the session to the NEAR account id (signed message, NEP-413 style).
- Acceptance: user connects, sees their account id in the header, reload keeps the session.

### F2. Villages and live data (P0)
- F2.1 At least 4 villages at launch, all from Rhea (lending markets for at least 2 assets, and at least 2 DEX pools). Exact pools are chosen on day 1 based on TVL and data availability.
- F2.2 Indexer fetches each village on a configurable interval (default 1 hour, HANDOFF D19) and stores a snapshot when the source has a new point: base APY, reward APY, TVL, source timestamp. Source: DefiLlama (D13).
- F2.3 Each village shows its current APR and 7-day APR sparkline on the map.
- F2.4 If a data source fails, the village shows "stale" with the age of the last value, and agents treat it as unavailable.
- Acceptance: open the town, each village shows its latest APY and is not stale (fetched successfully within the stale threshold, default 2 hours, D20). The UI says the data is updated hourly.

### F3. Character creation (P0)
- F3.1 Form fields: name, sprite (pick from 6 to 10), risk level (low, medium, high), goal (one of: steady income, max yield, protect capital), free-text rules (max 500 characters), starting capital (sim only, fixed options).
- F3.2 Risk level maps to hard limits in code (section 6). The LLM cannot change them.
- F3.3 On create, the character goes straight into the town. No backtest is needed (D51); showing one is optional (F5).
- Acceptance: create a character and see it appear in the town.

### F4. Agent decision loop (P0)
- F4.1 Each tick, for each active character: build the market state (all villages, APRs, 24h change, its own positions), send it with the persona to the LLM, get back a JSON action.
- F4.2 The action is validated against the guardrails (section 6). Invalid action becomes `HOLD`, and the reason is logged.
- F4.3 The LLM also returns a short public reason (max 140 characters) shown in the UI. The persona itself is never shown.
- F4.4 Low temperature (0 to 0.2) for decisions.
- F4.5 LLM provider: NEAR AI Cloud (OpenAI-compatible). Fallback: any OpenAI-compatible provider via config.
- Acceptance: run 3 ticks with 5 characters, every decision is logged with input hash, action, validity and reason.

### F5. Backtest (P2, optional)
**Optional (D51):** no other feature depends on the backtest. Build or finish it only if there is time. The engine exists in `backend/src/backtest/` (mock decision rule, not shown in the UI); switching it to the LLM (D35) is on hold.
- F5.1 Replay the last 7 days of daily (or finer) APR history for the chosen villages.
- F5.2 Use the same decision function as live mode, but to control LLM cost, run one decision per day of history (7 calls max per character, D60).
- F5.3 Apply the same cost model as simulation (F6.3).
- F5.4 Mark all backtest results with a "backtest" label everywhere.
- Acceptance (only if built): a new character has 7 daily equity points labeled backtest.

### F6. Simulation mode (P0)
- F6.1 Paper balance in USD terms, stored in the database.
- F6.2 Yield accrues every tick from the stored APY (HANDOFF D21): `position_value *= (1 + apy / 100) ^ (tick_seconds / 31_536_000)`. One day is `(1 + apy / 100) ^ (1 / 365)`.
- F6.3 Every move pays costs: swap fee of the pool used, slippage estimate (based on size vs pool TVL), and gas converted to USD. Costs are shown per move.
- Acceptance: a character that moves often ends with lower equity than one with the same APR exposure that holds, all else equal (unit test).

### F7. Real mode (P1)
- F7.1 User deposits a supported stablecoin into the Fondouk vault contract for a specific character.
- F7.2 Agent decisions for real characters are executed by the backend through the vault, only with these actions: supply to a whitelisted Rhea Lend market, withdraw from it back to the vault.
- F7.3 Only the owner can withdraw from the vault to their own account.
- F7.4 Hard caps: max deposit per character (for example 50 USD equivalent during the hackathon), max amount per action, cooldown between actions.
- F7.5 If the vault integration with Rhea is not working by day 9, real mode ships as "deposit, hold in vault, withdraw" only, and the README says so clearly.
- Acceptance: deposit, see the agent supply to Rhea (tx link shown), withdraw back to owner wallet.

### F8. Town view (P0)
- F8.1 Tile map with villages, paths, and an arena square.
- F8.2 Characters are sprites that walk to the village where most of their funds are.
- F8.3 Click a character: panel with owner, mode badge, equity chart, last 10 decisions with reasons, current positions.
- F8.4 Click a village: panel with APR history, TVL, characters inside.
- F8.5 Live updates via WebSocket, no page reload.
- Acceptance: when a decision moves a character, its sprite walks to the new village within 5 seconds.

### F9. Social arena (P1)
- F9.1 After each tick, a few characters can post a short message to the arena (action `SAY`), shown as speech bubbles and in a feed.
- F9.2 Messages are generated from public info only (APRs, own public reason). They must never contain the persona text.
- F9.3 Rate limit: at most 1 message per character per hour, at most 20 messages per hour in total.
- P2: characters can "listen" to the feed as an extra input to their decision.
- Acceptance: the arena feed shows messages, none contain persona text (automated check: no 8-word overlap with the persona).

### F10. Leaderboard (P0)
- F10.1 Columns: name, owner, mode badge (sim, real), 7d return, 30d return, max drawdown, number of moves, age, clone count, price.
- F10.2 Default sort: 30d return, but characters younger than 7 days of live history are shown in a separate "new" tab.
- F10.3 Return formula: `equity_end / equity_start - 1` for the window. Drawdown: max peak-to-trough drop of equity in the window.
- Acceptance: numbers on the leaderboard match a manual calculation from the equity table for 3 sample characters.

### F11. Marketplace and cloning (P0)
- F11.1 A creator can list a character with a price in NEAR. Listing is an on-chain call by the creator's wallet.
- F11.2 A buyer pays exactly the price to the marketplace contract. The contract sends 90% to the creator and 10% to the platform account in the same transaction, and emits an event.
- F11.3 Backend watches events, creates a clone for the buyer that references the source config. The buyer never receives the persona text.
- F11.4 A clone starts with a new balance (sim or real) and its own track record. Its listing page links to the source.
- F11.5 Clones cannot be re-listed (prevents reselling someone else's strategy).
- Acceptance: on testnet, buy a clone, see creator balance increase by 90% of the price, and see the clone appear in the buyer's list.

### F12. Demo mode (P0)
- F12.1 A seeded town with 8 to 12 characters of different styles, running live (`sim`) **as early as possible** (as soon as `LLM_*` is set), so they have days of real history by the demo (D51).
- F12.2 A "Judge tour" button: a 5-step guided overlay (town, character panel, leaderboard, clone flow, vault safety).
- Acceptance: a fresh visitor can complete the tour without connecting a wallet.

---

## 5. Action set

The LLM must return JSON matching one of these. Anything else is rejected and becomes `HOLD`.

```json
{ "action": "HOLD", "reason": "..." }
{ "action": "MOVE", "from": "village_id", "to": "village_id", "percent": 25, "reason": "..." }
{ "action": "REBALANCE", "weights": { "village_a": 60, "village_b": 40 }, "reason": "..." }
{ "action": "SAY", "text": "...", "reason": "..." }
```

Rules: `percent` is 1 to 100. `weights` sum to 100 and use only allowed villages. `reason` max 140 characters. `SAY` does not change positions.

---

## 6. Guardrails (enforced in code, not in the prompt)

| Rule | Low risk | Medium risk | High risk |
| --- | --- | --- | --- |
| Allowed village kinds | Stablecoin lending only | Lending + stable LP | All |
| Max percent moved per tick | 25% | 50% | 100% |
| Cooldown between moves | 6 ticks | 3 ticks | 1 tick |
| Min APR gain to move (after costs, annualized) | 1.5 points | 1.0 point | 0.5 point |
| Max share in one village | 60% | 80% | 100% |

Global rules:
- A move is rejected if its estimated cost is larger than 7 days of the expected APR gain.
- Stale villages cannot receive funds. Stale = no successful indexer fetch for longer than `VILLAGE_STALE_AFTER_SECS` (default 2 x the fetch interval = 2 hours, HANDOFF D20).
- In real mode, the vault contract enforces its own on-chain limits as well (see `TECH_architecture.md`). The backend checks are the first line, the contract is the last line.

---

## 7. Data shown to users (honesty rules)

- Backtest, simulated and real results are always labeled. They are never merged into one number.
- Every return is shown together with max drawdown and number of moves.
- Characters with less than 7 days of live history are marked "new".
- The UI never says "guaranteed", "safe yield" or similar.
- A short risk note is shown before any real deposit.

---

## 8. Success metrics for the demo

- Time from landing to first character in town: under 2 minutes.
- At least 10 characters live in the town during judging, at least 1 in real mode.
- At least 1 real clone purchase on testnet and 1 real Rhea supply transaction, with links in the README.
- Zero persona leaks in the arena feed (automated check).

---

## 9. Open questions

| # | Question | How we answer it | Owner | Deadline |
| --- | --- | --- | --- | --- |
| Q1 | Is DefiLlama yield history for Rhea pools free and complete? | Call the API for the chosen pools | Ayoub | **Answered 2 Nov: free, daily points, see HANDOFF U1** |
| Q2 | Does Rhea have a testnet deployment? If not, real mode is mainnet with tiny caps. | Check guide.rhea.finance and the Rhea GitHub | Ayoub | 1 Nov |
| Q3 | Exact Rhea Lend contract methods for supply and withdraw from another contract. | Read Rhea docs and contract source | Ayoub | 3 Nov |
| Q4 | Which NEAR AI Cloud model gives reliable JSON at low cost? | Test 2 to 3 models with the action schema | Ayoub | 2 Nov |
| Q5 | Clone fee currency: NEAR only, or also a stablecoin? | NEAR only for MVP unless trivial | Ayoub | 5 Nov |
| Q6 | Final track list for submission | Decide on 13 Nov based on what shipped | Team | 13 Nov |
