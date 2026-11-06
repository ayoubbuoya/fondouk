-- Wallet login (dev-docs/2026-11-02/database-design.md, section 2.4;
-- HANDOFF D37, D38).

-- NEP-413 challenge nonces. A row is deleted the moment it is used, before
-- the signature is checked, so one signed message can log in only once (no
-- replay). Unused rows expire after a few minutes and are cleaned up when
-- new challenges are created.
CREATE TABLE auth_challenges (
    nonce      BYTEA PRIMARY KEY CHECK (octet_length(nonce) = 32),
    expires_at TIMESTAMPTZ NOT NULL
);

-- Server-side sessions (Q-G): deleting a row logs the browser out.
-- Only the sha256 of the cookie token is stored, so a database leak does not
-- hand out working session cookies.
CREATE TABLE sessions (
    token_hash BYTEA PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    account_id TEXT NOT NULL REFERENCES users (account_id),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Serves the foreign key check and "every session of this account".
CREATE INDEX sessions_account_id_idx ON sessions (account_id);
