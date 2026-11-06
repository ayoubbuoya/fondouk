-- The exact error text when the LLM gave no usable action (rejected_rule
-- 'llm_error' or 'parse_error'): one line per attempt. Before this, the text
-- only reached the console logs, so a failure (e.g. a provider 429) was lost
-- once the logs scrolled away.
--
-- Internal only: never returned by the API or sent on /ws (the decisions
-- endpoint is public, and provider errors can name quotas or account details).
-- Never holds the model's own answer text (it may echo the private persona).
-- 8000 characters fits any JSON API error for both attempts; Rust cuts longer
-- text (an HTML error page) before inserting, so this CHECK never fails a tick.
ALTER TABLE decisions
    ADD COLUMN error_detail TEXT NULL CHECK (char_length(error_detail) <= 8000);
