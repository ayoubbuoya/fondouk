-- Day 4: live sim positions and the data the cost model needs (HANDOFF D31).

-- Pool fee and accepted tokens per village. Values checked on-chain on
-- 6 Nov 2026 (v2.ref-finance.near get_pool, contract.main.burrow.near):
--   rhea-dex-stable4   = Rhea pool 4179, RATED_SWAP, total_fee 2 (bps)
--   rhea-dex-frax-usdc = Rhea pool 4514, RATED_SWAP, total_fee 2 (bps)
--   Rhea Lend (Burrow) charges no supply or withdraw fee.
-- `tokens` uses short symbols; the cost model only compares them.
ALTER TABLE villages
    ADD COLUMN fee_bps INT    NOT NULL DEFAULT 0 CHECK (fee_bps BETWEEN 0 AND 10000),
    ADD COLUMN tokens  TEXT[] NOT NULL DEFAULT '{}';

UPDATE villages SET fee_bps = 0, tokens = '{USDC}'                      WHERE id = 'rhea-lend-usdc';
UPDATE villages SET fee_bps = 0, tokens = '{USDT}'                      WHERE id = 'rhea-lend-usdt';
UPDATE villages SET fee_bps = 2, tokens = '{USDT,USDC,USDT.e,USDC.e}'   WHERE id = 'rhea-dex-stable4';
UPDATE villages SET fee_bps = 2, tokens = '{FRAX,USDC}'                 WHERE id = 'rhea-dex-frax-usdc';

-- Every village must accept at least one token, otherwise money can never
-- enter it. Added after the UPDATEs so the seed rows pass.
ALTER TABLE villages
    ALTER COLUMN tokens DROP DEFAULT,
    ADD CONSTRAINT villages_tokens_not_empty CHECK (cardinality(tokens) > 0);

-- How much of a live (`sim`) avatar's money sits in each village. Cash is in
-- `avatars.cash_usd`. Same rule as the in-memory ledger: a village with no
-- money has no row, so value_usd is always > 0.
-- The backtest keeps its positions in memory and never writes here.
CREATE TABLE positions (
    avatar_id  UUID    NOT NULL REFERENCES avatars (id),
    village_id TEXT    NOT NULL REFERENCES villages (id),
    value_usd  NUMERIC NOT NULL CHECK (value_usd > 0),
    PRIMARY KEY (avatar_id, village_id)
);
