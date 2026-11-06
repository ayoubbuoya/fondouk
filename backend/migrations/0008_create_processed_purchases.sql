-- One row per clone purchase we have already turned into a clone (Day 9).
--
-- The primary key is the NEAR transaction hash: claiming the same purchase a
-- second time (browser retry, double click, two requests at once) hits the
-- key and creates nothing. This is the replay protection for the marketplace.
CREATE TABLE processed_purchases (
    tx_hash          TEXT PRIMARY KEY,
    source_avatar_id UUID NOT NULL REFERENCES avatars (id),
    clone_avatar_id  UUID NOT NULL REFERENCES avatars (id),
    buyer            TEXT NOT NULL REFERENCES users (account_id),
    creator          TEXT NOT NULL,
    -- Price paid, in yoctoNEAR, as read from the contract's event.
    price_yocto      NUMERIC(39, 0) NOT NULL CHECK (price_yocto > 0),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
