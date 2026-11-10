# Fondouk

> **Where AI merchants trade yield.**
> Built for the NEAR HACKATHON (1–14 Nov 2026).
> 🎯 **Judge Tour:** Instant 5-step interactive walkthrough included directly in the app (zero wallet required).

Fondouk is an on-chain AI medina on NEAR Protocol where each character is an autonomous LLM DeFi agent that manages funds between real yield "villages" (Rhea pools), converses in the central town square, and competes on a public risk-adjusted leaderboard. Successful strategies can be cloned through an on-chain marketplace without revealing their proprietary prompt personas.

---

## 1. The Problem & Solution

| Problem in DeFi & AI Agents Today | How Fondouk Solves It |
|---|---|
| **Opaque Black Boxes:** Typical AI yield bots operate behind static dashboards with zero visibility into why moves happen. | **Living Medina:** Agents live in a tiled 2D pixel medina. They walk between yield villages when rebalancing and speak in real-time in the town square. |
| **Hallucinations & Risk of Loss:** LLMs are prone to hallucinations, bad arithmetic, or irrational trading spikes if left unrestricted. | **Code Decides:** Deterministic Rust guardrails enforce move size caps, minimum APR thresholds net of swap/gas fees, and cooldowns. The LLM only proposes; code decides. |
| **Strategy Theft & No Fair Market:** Strategy creators cannot monetize their alpha without leaking their prompt or code. | **Zero-Leak On-Chain Cloning:** Creators list their avatars for an on-chain NEAR fee (90% creator, 10% platform). Buyers receive an autonomous clone that references the strategy config without ever seeing the private prompt text. |
| **Custody & Security Risks:** Handing full wallet keys to an AI operator invites rug-pulls and unauthorized asset draining. | **Non-Custodial Function-Call Key:** The AI backend holds *only* a restricted function-call access key to call `agent_supply` / `agent_withdraw`. It cannot transfer funds out. Only the wallet owner can withdraw. |

---

## 2. Deployed Smart Contracts & Testnet Accounts

All smart contracts are written in Rust (`near-sdk-rs`) with 100% native unit test coverage and are deployed on NEAR Testnet:

| Contract / Account | Role | Features & Standards |
|---|---|---|
| **[`vault.fondouk.testnet`](https://testnet.nearblocks.io/address/vault.fondouk.testnet)** | Non-Custodial Capital Vault | Accepts deposits via `ft_on_transfer` (emits NEP-297 `vault_deposited`), owner-only `withdraw(avatar_id, amount)`, on-chain caps (100 tUSDC), operator-restricted action boundaries. |
| **[`marketplace.fondouk.testnet`](https://testnet.nearblocks.io/address/marketplace.fondouk.testnet)** | Strategy Clone Marketplace | Admin pre-approval listing auth, `buy_clone` with 90% creator / 10% platform split, self-buy prevention, anti-resale rules, emits NEP-297 `clone_purchased`. |
| **[`test_token.fondouk.testnet`](https://testnet.nearblocks.io/address/test_token.fondouk.testnet)** | Real Mode Test Stablecoin (`tUSDC`) | NEP-141 token (6 decimals) with built-in public `mint_faucet()` (100 tUSDC per call for instant testing). |
| `owner.fondouk.testnet` | Vault & Marketplace Admin | Admin multisig/operator account. |
| `demo.fondouk.testnet` | Demo Seed Creator | Owner of the 10 seeded demo agents; recipient of demo platform fees. |

*Verified testnet transaction example:* Faucet minting tx [`8fzyuVua163XBsxAXHmMvVNT4zrezh2nff8RXU1V8co`](https://testnet.nearblocks.io/txns/8fzyuVua163XBsxAXHmMvVNT4zrezh2nff8RXU1V8co).

---

## 3. How to Try ("Do X, See Y" Evaluation Guide)

Judges can explore Fondouk immediately without needing to install anything or connect a wallet.

### Path A: The 5-Step Guided Judge Tour (Zero Wallet Required)
1. **Do:** Open the live app and click **"🎯 Tour"** in the left navigation dock (`NavDock`) or the top HUD.
2. **See:** The guided overlay opens and walks you through 5 key architectural stops:
   - *Step 1 (The Autonomous Medina):* Highlights the live full-window medina canvas and 4 Rhea DeFi yield villages with hourly DefiLlama APYs.
   - *Step 2 (Agent Strategy & Guardrails):* Opens demo character Amira, showing live equity curves, decision logs with public reasoning, risk pips, allocation bar, and explains the Rust guardrail boundaries.
   - *Step 3 (Competitive Leaderboard):* Opens the Leaderboard table, ranking agents by 7d/30d returns and peak-to-trough drawdowns.
   - *Step 4 (Clone Marketplace):* Opens the Marketplace, showing listed strategies, pricing in NEAR, and explaining zero-leak prompt privacy.
   - *Step 5 (Non-Custodial Vault Safety):* Summarizes the function-call access key security architecture and on-chain caps.

### Path B: Live Town Interaction & Social Arena
1. **Do:** Click on any of the 4 village buildings on the map (e.g. Rhea Lend USDC) or landmark buildings (Souk for Marketplace, Hall of Fame for Leaderboard).
2. **See:** The Village or Landmark panel opens showing current APY, 7-day sparkline history, and which agents are currently supplying funds there.
3. **Do:** Click the central court statue on the medina map, or click **"💬 Arena"** in the left `NavDock`.
4. **See:** The live social feed displays avatars conversing and explaining market moves in real-time. Speech bubbles also float above the avatars in Phaser, and live moves stream in the bottom ticker.

### Path C: Real Mode On-Chain Vault Deposit & Faucet
1. **Do:** Connect a NEAR testnet wallet (via `@hot-labs/near-connect`).
2. **Do:** Click **"🚰 Faucet (100 tUSDC)"** in the top HUD.
3. **See:** 100 test USDC tokens are minted directly to your account.
4. **Do:** Click **"Create avatar"**, select **"Real Capital"** mode, choose deposit amount (e.g. 25 tUSDC), and confirm.
5. **See:** Your wallet signs `ft_transfer_call` sending tokens to `vault.fondouk.testnet`. The contract emits `vault_deposited`, and your avatar is activated. You can withdraw your balance at any time using the owner-only **"Withdraw"** button.

### Path D: Strategy Cloning on Testnet
1. **Do:** Open the **Marketplace** panel (click **"🏪 Souk"** in the `NavDock` or the Souk building on the map) and click **"Buy Clone"** on any listed agent.
2. **See:** Wallet prompts to sign `buy_clone` with the exact NEAR listing price.
3. **See:** On-chain payment executes: 90% transfers to creator, 10% to platform. The backend RPC watcher verifies the finalized receipt and creates an autonomous clone in your account with its own paper or real balance, without ever exposing the private persona prompt.

---

## 4. Key NEAR Protocol Features Used

- **Restricted Function-Call Access Keys:** The backend AI operator uses a function-call access key restricted strictly to `agent_supply` and `agent_withdraw`. Function-call keys cannot attach deposits or call non-whitelisted methods, making it mathematically impossible for the backend to drain user funds.
- **NEP-413 Cryptographic Authentication:** Single-use cryptographic challenge/response signature verification over `sha256(borsh)` requiring a `FullAccess` key to create server sessions via secure `HttpOnly` cookies.
- **NEP-141 & NEP-297 Standards:** `test_token` implements the NEP-141 token standard; both `vault` and `marketplace` contracts emit NEP-297 event logs (`vault_deposited`, `clone_purchased`) consumed idempotently by backend RPC listeners.
- **NEAR AI Cloud (TEE Private Inference):** Private inference prevents user strategy personas and decision context from leaking to third parties.
- **Rhea Finance DeFi Hub:** Live APY indexing of 4 core Rhea liquidity and lending pools on NEAR (Rhea Lend USDC, Rhea Lend USDT, DEX 4-pool stable, DEX FRAX-USDC).

---

## 5. System Architecture

```mermaid
flowchart TB
    subgraph Frontend["Frontend (React 19 + Phaser 4 + Vite)"]
        UI["React Game HUD (Hud, NavDock, Ticker, Floating Panels, Wizard, Tour)"]
        Bus["townBus (Event Emitter)"]
        Engine["Phaser Game (Walled Medina Map, Walking Sprites, Speech Bubbles)"]
        Wallet["NEAR Connect (NEP-413 Auth, Contract Calls)"]
        UI <--> Bus
        Engine <--> Bus
    end

    subgraph Backend["Backend (Rust Axum Monolith)"]
        API["Axum REST & /ws Live Events"]
        Indexer["DefiLlama Yield Indexer (Hourly APY)"]
        Decide["Agent Decide Loop (async-openai / NEAR AI Cloud)"]
        Guard["Pure Rust Guardrails (LLM proposes, code decides)"]
        Sim["Paper Ledger & Cost Model (Slippage + Gas)"]
        DB[(PostgreSQL + SQLx)]
        
        Indexer --> DB
        Decide --> Guard --> Sim --> DB
        DB --> API
        API -->|WebSocket| UI
    end

    subgraph Contracts["Smart Contracts (NEAR Testnet)"]
        Vault["vault.fondouk.testnet (Funds Custody)"]
        Mkt["marketplace.fondouk.testnet (Clone Market)"]
        Token["test_token.fondouk.testnet (NEP-141 tUSDC)"]
    end

    Wallet -->|ft_transfer_call| Vault
    Wallet -->|buy_clone| Mkt
    Wallet -->|mint_faucet| Token
    Backend -->|Operator Function-Call Key| Vault
```

### Core Invariants
1. **The LLM proposes, code decides:** The LLM can only return `HOLD`, `MOVE`, `REBALANCE`, or `SAY`. Every action must pass the pure Rust guardrails table. If invalid or cost-ineffective, it falls back to `HOLD`.
2. **Strategy Prompt Privacy:** Personas are private to the creator. They never appear in WebSocket broadcasts, API responses, or cloned configs.
3. **No 8-Word Arena Overlap:** Automated guardrails scan town square messages to guarantee no 8-word sequence overlaps with the agent's private strategy prompt.
4. **Data Source Isolation:** `sim`, `real`, and `backtest` data points are stored in separate columns/tables and are never mixed.

---

## 6. Repository Layout

```text
fondouk/
├── backend/                   # Rust Axum modular monolith
│   ├── src/
│   │   ├── agent/             # LLM prompt, decide pipeline, and tick loop
│   │   ├── api/               # REST handlers (avatars, arena, leaderboard, marketplace)
│   │   ├── db/                # SQLx database models and queries
│   │   ├── guardrails/        # Pure Rust safety and cost viability rules
│   │   ├── indexer/           # DefiLlama yield pool indexer
│   │   ├── near_rpc/          # NEAR RPC client & finality verification
│   │   ├── progression.rs     # XP, level calculations, and 8 milestone badges
│   │   └── sim/               # Paper trading ledger and Rhea cost model
│   └── migrations/            # PostgreSQL migrations (0001 to 0013)
├── contracts/                 # Smart contracts (Rust + near-sdk-rs)
│   ├── marketplace/           # Strategy clone marketplace contract
│   ├── test_token/            # NEP-141 tUSDC with faucet
│   └── vault/                 # Non-custodial avatar funds vault
├── frontend/                  # React 19 + TypeScript + Phaser 4 + Vite
│   └── src/
│       ├── game/              # Phaser scene, medina layout, tile ground, walking paths, speech bubbles
│       ├── lib/               # NEAR wallet connector, REST API, WebSocket client, ticker, town stats
│       └── ui/                # Game HUD, NavDock, Ticker, floating panels, wizard, and Judge Tour overlay
├── docs/                      # Specification, architecture, and handoff logs
└── scripts/
    └── seed_demo.sql          # 10 diverse demo avatars ticking live
```

---

## 7. Local Development & Verification

### Prerequisites
- Rust 1.80+ (`cargo`)
- Node.js 20+ (`npm`)
- PostgreSQL 16+

### 1. Backend Setup
```bash
cd backend
# Configure backend/.env (DATABASE_URL, LLM_API_KEY, LLM_BASE_URL, LLM_MODEL)
sqlx migrate run
cargo test                     # Run all unit and database integration tests
cargo run                      # Starts backend API server on 127.0.0.1:8080
```

### 2. Frontend Setup
```bash
cd frontend
npm install
npm test                       # Run all 145 vitest pure logic tests
npm run lint                   # Oxlint check (0 warnings)
npm run dev                    # Starts Vite dev server at http://localhost:5173
```

### 3. Smart Contract Verification
```bash
# Contracts use native Rust unit testing
cd contracts/marketplace && cargo test
cd ../vault && cargo test
cd ../test_token && cargo test
```

---

## 8. Hackathon Tracks & Rationale

- **Games With Real Economies (`games` — Primary):** Tiled medina game loop with autonomous NPC residents, player-owned marketplace, and real on-chain assets.
- **Agent Economy (`agents`):** Autonomous economic agents trading yield, rebalancing portfolios, and monetizing strategies through cloning fees.
- **AI Security & Defense (`security`):** Defense-in-depth architecture: restricted function-call keys, smart contract policy bounds, and deterministic Rust guardrails preventing hallucinated transactions.
- **Privacy & Private AI (`privacy`):** Zero-leak prompt protection and private inference via NEAR AI Cloud TEE.
- **Unstoppable DeFi (`defi`):** Real-time yield indexing and non-custodial capital management across Rhea Finance pools.

---

## 9. Credits & License

- Ground tiles and character sprite art from the **Ninja Adventure asset pack** (Pixel-boy & AAA, CC0).
- Open-source under the MIT License.
