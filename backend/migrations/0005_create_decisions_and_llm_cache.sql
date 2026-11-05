-- Agent activity (dev-docs/2026-11-02/database-design.md, section 2.3).

-- One row per decision: what the LLM proposed and what code applied.
-- "The LLM proposes, code decides": a refused proposal is stored with the
-- rule that refused it, and the applied action is HOLD.
CREATE TABLE decisions (
    id              BIGSERIAL PRIMARY KEY,
    avatar_id       UUID NOT NULL REFERENCES avatars (id),
    ts              TIMESTAMPTZ NOT NULL,     -- historical day for backtest rows
    source          TEXT NOT NULL CHECK (source IN ('backtest', 'sim', 'real')),
    input_hash      TEXT NOT NULL,            -- sha256 of the market state sent (F4 acceptance)
    model           TEXT NOT NULL,            -- LLM model id that decided (D17)
    proposed_action JSONB NULL,               -- what the LLM asked for; NULL = no usable answer
    applied_action  JSONB NOT NULL,           -- what we did; HOLD when refused
    valid           BOOLEAN NOT NULL,
    rejected_rule   TEXT NULL,                -- 'parse_error', 'llm_error', 'max_share', ...
    reason_public   TEXT NULL CHECK (char_length(reason_public) <= 140),
    cost_usd        NUMERIC NOT NULL DEFAULT 0,
    tx_hash         TEXT NULL,                -- real mode only
    CHECK (valid = (rejected_rule IS NULL))
);
CREATE INDEX decisions_avatar_ts_idx ON decisions (avatar_id, ts DESC);

-- LLM answers by input, so re-running the same decision costs no new call
-- (keeps the backtest within 30 LLM calls per character).
CREATE TABLE llm_cache (
    cache_key  TEXT PRIMARY KEY,              -- sha256(model + system prompt + user prompt)
    response   JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
