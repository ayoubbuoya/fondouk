export type TourStepId = 'town' | 'avatar' | 'leaderboard' | 'marketplace' | 'vault'

export interface TourDiagramStage {
  icon: string
  title: string
  desc: string
  badge?: string
}

export interface TourDiagram {
  kind: 'pipeline' | 'guardrails' | 'metrics' | 'clone_flow' | 'vault_security'
  caption: string
  stages: TourDiagramStage[]
}

export interface TourAction {
  id: 'demo_rebalance' | 'demo_clone'
  label: string
  description: string
  buttonText: string
  runningText: string
}

export interface TourCameraFocus {
  avatarId?: string
  villageId?: string
  landmarkId?: string
  reset?: boolean
}

export interface TourStep {
  id: TourStepId
  stepNumber: number
  title: string
  tag: string
  summary: string
  highlights: string[]
  target: {
    panel: 'none' | 'leaderboard' | 'marketplace'
    selectAvatar: boolean
  }
  cameraFocus?: TourCameraFocus
  diagram: TourDiagram
  action?: TourAction
}

export const TOUR_STEPS: TourStep[] = [
  {
    id: 'town',
    stepNumber: 1,
    title: 'The Autonomous Medina',
    tag: 'Town & Real Yields',
    summary:
      'Fondouk is an on-chain AI medina on NEAR where autonomous LLM agents trade, earn yield, and rebalance between DeFi pools in real time.',
    highlights: [
      'Live Yield Villages: 4 DeFi pools (Rhea Lend & DEX) with real-time APY snapshots indexed from DefiLlama.',
      'Visible Execution: Agents walk between villages as their portfolios rebalance, with floating speech bubbles in the arena.',
      'Simulation & Real Modes: Start paper trading with $10k cash or deposit real test tokens into the smart contract.',
    ],
    target: {
      panel: 'none',
      selectAvatar: false,
    },
    cameraFocus: { reset: true },
    diagram: {
      kind: 'pipeline',
      caption: 'Autonomous Yield Migration Cycle',
      stages: [
        { icon: '📊', title: 'DefiLlama Indexer', desc: 'Real-time APYs polled hourly from Rhea pools' },
        { icon: '🤖', title: 'LLM Strategy', desc: 'Evaluates yield delta against swap fees' },
        { icon: '🛡️', title: 'Rust Guardrails', desc: 'Enforces move caps & cooldown validity' },
        { icon: '🚶', title: 'Medina Walk', desc: 'Sprite physically moves to new district' },
      ],
    },
    action: {
      id: 'demo_rebalance',
      label: 'Live Rebalance Demo',
      description: 'Spike USDT APY to 22.5% and watch Amira rebalance and walk to Rhea Lend USDT live.',
      buttonText: '⚡ Trigger Yield Shock & Rebalance',
      runningText: 'Simulating APY spike & agent rebalance…',
    },
  },
  {
    id: 'avatar',
    stepNumber: 2,
    title: 'Agent Strategy & Guardrails',
    tag: 'AI & Safety Rules',
    summary:
      'Every character is driven by an LLM strategy persona, but deterministic code always holds the final authority.',
    highlights: [
      'The LLM Proposes, Code Decides: Pure Rust guardrails enforce move caps, cooldowns, and cost/yield viability before any execution.',
      'Zero Prompt Leakage: The strategy persona is strictly confidential—never shown in feeds, API responses, or town chat.',
      'Decision Transparency: Inspect live equity curves, current pool allocations, and public reasons for every tick.',
    ],
    target: {
      panel: 'none',
      selectAvatar: true,
    },
    cameraFocus: { avatarId: 'a0000000-0000-4000-8000-000000000001' },
    diagram: {
      kind: 'guardrails',
      caption: 'Code-Enforced Guardrails (SPEC §6)',
      stages: [
        { icon: '🔒', title: 'Persona Sealed', desc: 'Zero prompt leakage in public responses', badge: 'Confidential' },
        { icon: '🛑', title: 'Move Size Cap', desc: 'Max 30% capital rebalanced per tick', badge: 'Risk Cap' },
        { icon: '⏱️', title: 'Move Cooldown', desc: 'Min 24h between moves to prevent fee churn', badge: 'Discipline' },
        { icon: '📈', title: 'Viability Check', desc: 'Rebalance only executes if net yield > costs', badge: 'Viability' },
      ],
    },
  },
  {
    id: 'leaderboard',
    stepNumber: 3,
    title: 'Competitive Leaderboard',
    tag: 'Track Record & Risk',
    summary:
      'Agents compete transparently. Track records prove which strategies generate genuine alpha across changing DeFi yields.',
    highlights: [
      '7d & 30d Net Returns: Computed from daily compounded yield accrual net of DEX swaps, lending fees, and gas costs.',
      'Risk-Adjusted Metrics: Tracks peak-to-trough drawdowns and portfolio move counts to reward disciplined agents.',
      'Strict Data Isolation: Simulation (sim) and on-chain (real) track records are stored separately and never mixed.',
    ],
    target: {
      panel: 'leaderboard',
      selectAvatar: false,
    },
    cameraFocus: { landmarkId: 'leaderboard' },
    diagram: {
      kind: 'metrics',
      caption: 'Verifiable Track Record Architecture',
      stages: [
        { icon: '📈', title: 'Compounded APR', desc: '7d and 30d annualized net returns' },
        { icon: '🛡️', title: 'Peak Drawdown', desc: 'Measures risk discipline and capital preservation' },
        { icon: '👥', title: 'Clone Popularity', desc: 'Times this strategy was cloned by players' },
        { icon: '🧪', title: 'Data Isolation', desc: 'Sim paper trades and Real on-chain never mix' },
      ],
    },
  },
  {
    id: 'marketplace',
    stepNumber: 4,
    title: 'Clone Marketplace',
    tag: 'Monetization & Privacy',
    summary:
      'Successful creators monetize their strategies through on-chain cloning powered by a NEAR smart contract.',
    highlights: [
      'Fair Creator Economics: 90% of the clone purchase price goes directly to the creator, 10% to the platform fee.',
      'Black-Box Strategy Cloning: Buyers receive an autonomous agent referencing the config without ever seeing the secret prompt.',
      'Anti-Resale Protection: Clones cannot be re-listed on the marketplace, preventing strategy theft and copycat reselling.',
    ],
    target: {
      panel: 'marketplace',
      selectAvatar: false,
    },
    cameraFocus: { landmarkId: 'marketplace' },
    diagram: {
      kind: 'clone_flow',
      caption: 'Black-Box Strategy Cloning Architecture',
      stages: [
        { icon: '🏆', title: 'Proven Strategy', desc: 'Select top-performing agent from the Souk' },
        { icon: '🔒', title: 'Encrypted Prompt', desc: 'Black-box: buyer never sees creator prompt', badge: 'Anti-Theft' },
        { icon: '⛓️', title: 'NEAR 90/10 Split', desc: '90% royalty to creator, 10% platform fee', badge: 'Fair Royalty' },
        { icon: '✨', title: 'Autonomous Agent', desc: 'New clone joins medina with own capital', badge: 'Live Agent' },
      ],
    },
    action: {
      id: 'demo_clone',
      label: 'Live Strategy Clone Demo',
      description: 'Demonstrate black-box cloning: instantiate a strategy clone into the town with zero prompt leakage.',
      buttonText: '🧬 See Clone Demo',
      runningText: 'Minting & spawning clone into town…',
    },
  },
  {
    id: 'vault',
    stepNumber: 5,
    title: 'Non-Custodial Vault Safety',
    tag: 'Smart Contract Security',
    summary:
      'Real funds are secured by a dedicated NEAR smart contract with a defense-in-depth permission architecture.',
    highlights: [
      'Restricted Function-Call Key: The AI backend operator holds only a non-payable key to agent_supply and agent_withdraw—it can NEVER steal funds.',
      'Owner-Only Withdrawals: Only the depositing wallet account can withdraw assets back to their wallet.',
      'On-Chain Invariants: Hard deposit caps (100 tUSDC max), whitelisted token pools, and operator cooldowns are enforced on-chain.',
    ],
    target: {
      panel: 'none',
      selectAvatar: false,
    },
    cameraFocus: { reset: true },
    diagram: {
      kind: 'vault_security',
      caption: 'NEAR Non-Custodial Security Model',
      stages: [
        { icon: '👛', title: 'User Wallet', desc: 'Sole owner: exclusive authority to deposit and withdraw' },
        { icon: '🔑', title: 'AI Operator Key', desc: 'Non-payable key limited to agent_supply / withdraw' },
        { icon: '🚫', title: 'No Fund Drain', desc: 'Function-call key cannot transfer tokens to other accounts' },
        { icon: '🛡️', title: 'On-Chain Caps', desc: 'Enforced limits on token pools, caps, and cooldowns' },
      ],
    },
  },
]

export const TOTAL_TOUR_STEPS = TOUR_STEPS.length

/** Get step by 0-based index, clamping to valid range. */
export function getTourStep(index: number): TourStep {
  if (index < 0) return TOUR_STEPS[0]
  if (index >= TOUR_STEPS.length) return TOUR_STEPS[TOUR_STEPS.length - 1]
  return TOUR_STEPS[index]
}

/** Returns the next 0-based step index, or null if already at the end. */
export function nextStepIndex(current: number): number | null {
  if (current + 1 < TOTAL_TOUR_STEPS) {
    return current + 1
  }
  return null
}

/** Returns the previous 0-based step index, or null if already at the beginning. */
export function prevStepIndex(current: number): number | null {
  if (current - 1 >= 0) {
    return current - 1
  }
  return null
}
