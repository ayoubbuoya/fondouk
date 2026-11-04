-- Characters tables (dev-docs/2026-11-02/database-design.md, section 2.2).
-- `positions` is not created here: the backtest keeps its ledger in memory,
-- and the live sim adds `positions` on Day 4.
--
-- Business rules are also CHECK constraints (design doc C9): even a Rust bug
-- cannot write an invalid row.

-- A NEAR account that uses Fondouk.
CREATE TABLE users (
    account_id TEXT PRIMARY KEY,                      -- NEAR account id
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- "The strategy". Rows are IMMUTABLE (Q-B): editing a strategy inserts a new
-- row, so a clone keeps exactly the version that was bought.
CREATE TABLE avatar_configs (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Private (Q-A): returned only to the owner, never in WebSocket, arena or clones.
    persona    TEXT NOT NULL CHECK (char_length(persona) BETWEEN 1 AND 2000),
    risk       TEXT NOT NULL CHECK (risk IN ('low', 'medium', 'high')),
    goal       TEXT NOT NULL CHECK (goal IN ('steady_income', 'max_yield', 'protect_capital')),
    created_by TEXT NOT NULL REFERENCES users (account_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- "The character" that runs a strategy.
CREATE TABLE avatars (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner                TEXT NOT NULL REFERENCES users (account_id),
    config_id            UUID NOT NULL REFERENCES avatar_configs (id),
    name                 TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 32),
    sprite               TEXT NOT NULL,
    mode                 TEXT NOT NULL CHECK (mode IN ('sim', 'real')),
    starting_capital_usd NUMERIC NOT NULL CHECK (starting_capital_usd > 0),
    cash_usd             NUMERIC NOT NULL CHECK (cash_usd >= 0),   -- money not in any village yet
    active               BOOLEAN NOT NULL DEFAULT true,           -- the tick loop runs only active ones
    cloned_from          UUID NULL REFERENCES avatars (id),
    listed               BOOLEAN NOT NULL DEFAULT false,          -- cache of the on-chain listing (Q-H)
    price_yocto          NUMERIC(39, 0) NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (cloned_from IS NULL OR listed = false),                -- F11.5: clones cannot be listed
    CHECK (listed = false OR price_yocto > 0)
);
