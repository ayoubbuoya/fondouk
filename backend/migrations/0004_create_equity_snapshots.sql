-- Track record table (dev-docs/2026-11-02/database-design.md, section 2.3).
--
-- One equity value per avatar, per data source, per time. Backtest, sim and
-- real results are labeled by `source` and never merged (CLAUDE.md, SPEC §7).
CREATE TABLE equity_snapshots (
    avatar_id  UUID NOT NULL REFERENCES avatars (id),
    source     TEXT NOT NULL CHECK (source IN ('backtest', 'sim', 'real')),
    ts         TIMESTAMPTZ NOT NULL,
    equity_usd NUMERIC NOT NULL CHECK (equity_usd >= 0),
    -- Matches the "avatar X, source Y, time range" queries (design doc C5).
    PRIMARY KEY (avatar_id, source, ts)
);
