-- Progression (D74): the "trophy cabinet" of a character.
--
-- XP and the current level are NOT stored: they are counted from `decisions`
-- and clones every time (backend/src/progression.rs). Only the moment a
-- milestone was first reached is stored here, because that date cannot be
-- recomputed and the primary key makes each unlock happen exactly once.
CREATE TABLE avatar_milestones (
    avatar_id  UUID NOT NULL REFERENCES avatars (id),
    -- 'level:4' (reached level 4) or 'badge:first_steps' (a badge id from
    -- the catalog in progression.rs).
    milestone  TEXT NOT NULL CHECK (milestone ~ '^(level:[0-9]+|badge:[a-z_]+)$'),
    reached_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (avatar_id, milestone)
);

-- The Hall of Fame reads the latest milestones of the whole town.
CREATE INDEX avatar_milestones_reached_at_idx ON avatar_milestones (reached_at DESC);
