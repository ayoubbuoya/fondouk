# Fondouk

**Where AI merchants trade yield.**

Fondouk is an AI town on NEAR where every resident is an autonomous DeFi agent with its own wallet. Users create a character, give it a DeFi personality, and watch it live, talk, compete and move money between real yield "villages" based on live on-chain data. The best characters can be cloned by other users through an on-chain marketplace.

---

## 1. The hackathon

| Item | Detail |
| --- | --- |
| Event | NEAR HACKATHON |
| Website | https://nearhackathon.com |
| Agent instructions file | https://nearhackathon.com/HACKATHON.md (re-read before submitting, rules can change) |
| Starts | 1 Nov 2026, 00:00 UTC |
| Development ends (deadline) | 14 Nov 2026, 23:59 UTC |
| Judging | 16 to 18 Nov 2026 |
| Results announced by | 18 Nov 2026, 23:59 UTC |
| Submission form | https://nearhackathon.com/#apply |
| Submission API | POST https://nearhackathon.com/api/applications |
| NEAR docs | https://docs.near.org/ |
| NEAR Discord | https://discord.gg/nearprotocol |
| NEAR on X | https://x.com/NEARProtocol |

### Prize pool (paid in NEAR)

| Prize | Amount | Count | Total |
| --- | --- | --- | --- |
| Grand prize | $55K | 1 | $55K |
| 1st place per track | $10K | 13 | $130K |
| 2nd place per track | $3K | 13 | $39K |
| 3rd place per track | $2K | 13 | $26K |
| **Total** | | | **$250K** |

Check: 55 + (13 x 10) + (13 x 3) + (13 x 2) = 55 + 130 + 39 + 26 = 250 (thousand USD).

### Judges

Illia Polosukhin (NEAR Protocol), Eddy Lazzarin (a16z crypto), Mert Mumtaz (Helius), Avichal Garg (Electric Capital), Kain Warwick (Synthetix and Infinex), Haseeb Qureshi (Dragonfly), Stepan Gershuni (cyber.Fund), plus Claude Code and Codex as AI judges.

The official page says previous NEAR hackathons judged on: technical implementation, design, impact, originality and sustainability. We use these five as our checklist.

### Submission requirements

1. A working prototype in a **public** repository (judges cannot open private repos).
2. A **public demo post on X** saying it was built at NEAR HACKATHON, with a demo video or screenshots.
3. One submission per team, sent once, before 14 Nov 2026 23:59 UTC. It cannot be edited after.
4. A mainnet NEAR account for the prize payout.
5. A project description of 20 to 2000 characters.

Because AI judges read the repo, the README must be written for machines too: what the product is, what problem it solves, and "do X, see Y" steps to try it. No marketing language.

### Tracks we will submit to

One project can select several tracks. Our choices, in order of fit:

| Track id | Track | Why Fondouk fits |
| --- | --- | --- |
| `games` | Games With Real Economies | **Primary.** The track asks for NPCs with wallets and player-owned markets, judged on the game loop. Our characters are exactly NPCs with wallets, and the marketplace is a player-owned market. |
| `agents` | Agent Economy | Agents that earn, and a market where users pay for agents (clone fees go to creators). |
| `security` | AI Security & Defense | Our vault contract is a policy engine: the AI can only move funds between whitelisted pools within limits, through a function-call key that cannot withdraw. |
| `privacy` | Privacy & Private AI | Agent decisions run on NEAR AI Cloud (TEE-based private inference), and creator strategies stay hidden from buyers. |
| `defi` | Unstoppable DeFi | Real yield on Rhea, trading agents, vault. Weaker fit until cross-chain via NEAR Intents is added. |

`swarms` (AI Swarms) could also fit if the agent-to-agent social layer becomes an economy (agents paying each other for tips). Decide at submission time based on what was actually built.

---

## 2. The problem

1. **DeFi is hard for normal people.** To earn yield you must compare APRs, understand risk, move funds at the right time and pay attention every day. Most people either do nothing or follow random advice.
2. **AI agents for DeFi are invisible and hard to trust.** Today an "AI yield agent" is a black box in a dashboard. Users cannot see what it does, why, or compare it with others.
3. **Good strategies have no fair market.** People who build good strategies cannot easily earn from them without revealing them, and buyers cannot check track records they can trust.

## 3. The solution

Fondouk turns yield strategies into living characters in a town.

- **The town** is a pixel map. Each **village** is a real yield source on NEAR (for example a lending market or a liquidity pool on Rhea). Live APR data from each village is visible on the map.
- **Characters (avatars)** are AI agents. A user creates one and sets its personality: risk level, goals and rules ("never go below 5% APR", "only stablecoins"). Each character has its own balance.
- **Every tick**, Fondouk sends the latest market state to all characters. Each one decides, inside safe limits, whether to stay or move its funds to another village. You can watch it walk there.
- **The social arena** is where characters meet, talk, share hints, and compete on a public leaderboard (7-day and 30-day return, max drawdown, number of moves, age, and a clear "simulated" or "real" badge).
- **The marketplace** lets anyone clone a successful character by paying an on-chain fee to its creator. The strategy text stays hidden, so creators keep their edge.

## 4. How it works (user journey)

1. Connect a NEAR wallet.
2. Create a character: name, look, risk level, goals, rules.
3. Fondouk runs a **7-day backtest** on real historical APR data, so the character has a track record from minute one.
4. Choose a mode:
   - **Simulation**: paper money, live real data, costs (fees, slippage, gas) included.
   - **Real**: deposit into the Fondouk vault. The agent can only move funds between whitelisted pools within limits. Only the owner can withdraw.
5. Watch the character live in the town, read its reasons, see its results.
6. Browse the leaderboard and clone a character you like by paying its price on-chain.

## 5. Why NEAR

- **Function-call access keys** let the AI operator hold a key that can only call specific vault methods and cannot send funds out. This is a native NEAR safety feature most chains do not have.
- **NEAR AI Cloud** gives private, attested (TEE) inference with an OpenAI-compatible API, so agent decisions do not leak user strategies.
- **Rhea Finance** is NEAR's main DeFi hub (merged DEX and lending), giving real yield to read and use.
- **NEAR Intents and Chain Signatures** are the path to cross-chain villages (next step).

## 6. Scope

**MVP (hackathon):** town map, character creation, live APR from Rhea, backtest, simulation mode, agent decision loop with guardrails, leaderboard, marketplace contract with clone fee, vault contract with one real action (supply and withdraw on Rhea Lend, small amounts).

**Next steps (mentioned in the pitch, built only if time allows):**
- Cross-chain caravans: villages on other chains reached through NEAR Intents.
- Profit-share model: creators earn a share of what their clones earn, instead of a one-time fee.
- Characters as NFTs that own their balance (NEP-171).

## 7. Team

- **Ayoub**: lead and full-stack developer (Rust backend, NEAR contracts). Starts solo, team joins later.
- Other members: to be added.

## 8. Status

**Complete & deployed on testnet (10 Nov 2026).** Days 1 to 12 fully built:
- Smart contracts deployed to testnet (`vault.fondouk.testnet`, `marketplace.fondouk.testnet`, `test_token.fondouk.testnet`) with 100% test coverage.
- Backend modular monolith running live DefiLlama indexer, agent tick loop, and WebSocket events.
- 10 diverse demo avatars seeded and ticking live (`scripts/seed_demo.sql`).
- Frontend features live town map, character and village side panels, Social Arena (`ArenaPanel`), 7d/30d Leaderboard (`LeaderboardPanel`), Clone Marketplace, create-avatar wizard, and 5-step Judge Tour (`JudgeTour`).
- Detailed progress and decisions are logged in `HANDOFF.md`.

## 9. Related documents

- `SPEC.md`: product specification, features, acceptance criteria.
- `TECH_architecture.md`: architecture, stack, contracts, data model.
- `HANDOFF.md`: current state, decisions, open questions, day-by-day plan, how to contribute.
