-- Social arena messages (SPEC F9, TECH §6, dev-docs 2026-11-02 database design).
-- Written when an avatar returns action SAY and passes all guardrails and rate limits.
CREATE TABLE arena_messages (
    id BIGSERIAL PRIMARY KEY,
    avatar_id UUID NOT NULL REFERENCES avatars(id),
    decision_id BIGINT NULL REFERENCES decisions(id),
    ts TIMESTAMPTZ NOT NULL,
    text TEXT NOT NULL CHECK (char_length(text) BETWEEN 1 AND 140)
);

-- Index for the public arena feed (ordered newest first) and total hourly rate limit.
CREATE INDEX arena_messages_ts_idx ON arena_messages (ts DESC);

-- Index for the per-character hourly rate limit (at most 1 message per character per hour).
CREATE INDEX arena_messages_avatar_ts_idx ON arena_messages (avatar_id, ts DESC);
