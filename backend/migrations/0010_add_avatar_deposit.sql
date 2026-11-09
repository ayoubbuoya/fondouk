-- Migration 0010: Track on-chain deposit transaction and token for Real Mode avatars.
ALTER TABLE avatars
    ADD COLUMN deposit_tx_hash TEXT NULL,
    ADD COLUMN deposit_token   TEXT NULL;
