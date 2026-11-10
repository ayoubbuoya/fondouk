-- Demo seed: 10 sim characters owned by one demo account (HANDOFF D51, D55).
--
-- Why: the leaderboard's 7d / 30d numbers come from live `sim` history only
-- (the backtest is optional, D51), so these characters must start ticking as
-- early as possible, days before the 14 Nov demo.
--
-- Run once against the dev or demo database (the tick loop picks them up at
-- its next round, if LLM_* is set):
--   psql "$DATABASE_URL" -f scripts/seed_demo.sql
--
-- Safe to run again: fixed ids + ON CONFLICT DO NOTHING, so nothing is
-- duplicated and existing characters (and their history) are never touched.
--
-- Personas are PRIVATE (D7, D39): only `demo.fondouk.testnet` can read them
-- through the API. They follow F3.1 (max 500 characters).
-- Sprites must be in the backend list `api::avatars::SPRITES` (D54).

BEGIN;

INSERT INTO users (account_id) VALUES ('demo.fondouk.testnet')
ON CONFLICT DO NOTHING;

INSERT INTO avatar_configs (id, persona, risk, goal, created_by) VALUES
    ('d0000000-0000-4000-8000-000000000001',
     'A careful saver. Keep most money in lending, where the rate is steady. Only move when another village pays clearly more for several days, never for a one-day spike.',
     'low', 'steady_income', 'demo.fondouk.testnet'),
    ('d0000000-0000-4000-8000-000000000002',
     'Protect the capital first. Prefer the village with the largest TVL. Avoid small pools. Moving costs money, so hold unless the gain is obvious.',
     'low', 'protect_capital', 'demo.fondouk.testnet'),
    ('d0000000-0000-4000-8000-000000000003',
     'Split the money between USDC and USDT lending so that no single stablecoin can hurt much. Rebalance only when one side drifts far from half.',
     'medium', 'protect_capital', 'demo.fondouk.testnet'),
    ('d0000000-0000-4000-8000-000000000004',
     'Follow the trend: when a village APY has been rising for a few days, move part of the money there. When it falls for a few days, leave.',
     'medium', 'max_yield', 'demo.fondouk.testnet'),
    ('d0000000-0000-4000-8000-000000000005',
     'A patient monk. Pick the best village once and stay. Move only if the current village becomes stale or its APY drops below half of the best one.',
     'low', 'steady_income', 'demo.fondouk.testnet'),
    ('d0000000-0000-4000-8000-000000000006',
     'A noble investor who likes liquidity pools. Keep a large share in the stable pool, the rest in lending. Explain each move like a banker.',
     'medium', 'steady_income', 'demo.fondouk.testnet'),
    ('d0000000-0000-4000-8000-000000000007',
     'Chase the highest yield. Go all in where the APY is best, including small pools, as long as the move pays for itself within a week.',
     'high', 'max_yield', 'demo.fondouk.testnet'),
    ('d0000000-0000-4000-8000-000000000008',
     'Fast and bold. React to every big APY change, but never move twice in a row in the same direction. Keep some cash only if every village looks bad.',
     'high', 'max_yield', 'demo.fondouk.testnet'),
    ('d0000000-0000-4000-8000-000000000009',
     'A contrarian. Distrust villages whose APY jumped suddenly; prefer the ones that have been steady for a week, even if they pay a bit less.',
     'medium', 'max_yield', 'demo.fondouk.testnet'),
    ('d0000000-0000-4000-8000-000000000010',
     'Balance yield and safety. Put about two thirds in lending and one third in the best pool. Hold when the difference is small.',
     'medium', 'steady_income', 'demo.fondouk.testnet')
ON CONFLICT DO NOTHING;

-- cash_usd = starting capital: sim characters start in cash and invest with
-- REBALANCE (D27). Capitals follow Q-E ($1k / $10k / $100k).
INSERT INTO avatars (id, owner, config_id, name, sprite, mode, starting_capital_usd, cash_usd, listed, price_yocto) VALUES
    ('a0000000-0000-4000-8000-000000000001', 'demo.fondouk.testnet', 'd0000000-0000-4000-8000-000000000001', 'Amira',   'princess',   'sim', 10000,  10000,  true, 500000000000000000000000),
    ('a0000000-0000-4000-8000-000000000002', 'demo.fondouk.testnet', 'd0000000-0000-4000-8000-000000000002', 'Old Habib', 'old_man',  'sim', 100000, 100000, true, 1000000000000000000000000),
    ('a0000000-0000-4000-8000-000000000003', 'demo.fondouk.testnet', 'd0000000-0000-4000-8000-000000000003', 'Karim',   'boy',        'sim', 10000,  10000,  true, 250000000000000000000000),
    ('a0000000-0000-4000-8000-000000000004', 'demo.fondouk.testnet', 'd0000000-0000-4000-8000-000000000004', 'Salma',   'hunter',     'sim', 10000,  10000,  true, 500000000000000000000000),
    ('a0000000-0000-4000-8000-000000000005', 'demo.fondouk.testnet', 'd0000000-0000-4000-8000-000000000005', 'Brother Yunus', 'monk', 'sim', 1000,   1000,   true, 100000000000000000000000),
    ('a0000000-0000-4000-8000-000000000006', 'demo.fondouk.testnet', 'd0000000-0000-4000-8000-000000000006', 'Lady Zaynab', 'noble',  'sim', 100000, 100000, true, 1000000000000000000000000),
    ('a0000000-0000-4000-8000-000000000007', 'demo.fondouk.testnet', 'd0000000-0000-4000-8000-000000000007', 'Tarek',   'samurai',    'sim', 10000,  10000,  true, 500000000000000000000000),
    ('a0000000-0000-4000-8000-000000000008', 'demo.fondouk.testnet', 'd0000000-0000-4000-8000-000000000008', 'Shadow Nour', 'ninja_blue', 'sim', 1000, 1000, true, 500000000000000000000000),
    ('a0000000-0000-4000-8000-000000000009', 'demo.fondouk.testnet', 'd0000000-0000-4000-8000-000000000009', 'Mehdi',   'boy',        'sim', 10000,  10000,  true, 250000000000000000000000),
    ('a0000000-0000-4000-8000-000000000010', 'demo.fondouk.testnet', 'd0000000-0000-4000-8000-000000000010', 'Ines',    'princess',   'sim', 10000,  10000,  true, 100000000000000000000000)
ON CONFLICT DO NOTHING;

COMMIT;
