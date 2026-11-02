-- Market data tables (dev-docs/2026-11-02/database-design.md, section 2.1).

-- A village is one yield source on the map (one Rhea pool or lending market).
CREATE TABLE villages (
    id              TEXT PRIMARY KEY,                 -- slug, e.g. 'rhea-lend-usdc'
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('lending', 'stable_lp')),
    protocol        TEXT NOT NULL,                    -- 'rhea'
    asset           TEXT NOT NULL,                    -- 'USDC', 'USDT-USDC-USDT.e-USDC.e'
    pool_ref        TEXT NOT NULL UNIQUE,             -- DefiLlama pool uuid (D13)
    map_x           INT  NOT NULL,                    -- placeholder until the Tiled map (Day 6)
    map_y           INT  NOT NULL,
    active          BOOLEAN NOT NULL DEFAULT true,
    -- Last successful indexer fetch. Drives the "stale" rule (D15), because the
    -- source refreshes about hourly and its own timestamp is not a health signal.
    last_fetched_at TIMESTAMPTZ NULL
);

-- One yield reading per village per SOURCE timestamp. The indexer inserts with
-- ON CONFLICT DO NOTHING, so re-fetching an unchanged point stores nothing new.
-- The backtest history import uses this same table.
CREATE TABLE yield_snapshots (
    village_id  TEXT NOT NULL REFERENCES villages (id),
    ts          TIMESTAMPTZ NOT NULL,
    apy_base    NUMERIC NOT NULL,                     -- percent per year, e.g. 14.01
    apy_reward  NUMERIC NOT NULL DEFAULT 0,
    tvl_usd     NUMERIC NOT NULL CHECK (tvl_usd >= 0),
    PRIMARY KEY (village_id, ts)
);
