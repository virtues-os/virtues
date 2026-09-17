-- reasoning_details: the gateway's normalized reasoning blocks for an
-- assistant turn (text plus provider signatures), stored so the turn can be
-- resent with its thinking intact when a model resumes after tool results.
--
-- Replaces thought_signature, the Gemini-specific field from 2026-06-08 that
-- never reached a model: the proxy re-typed the request without it and
-- dropped it on every turn until 2026-09-14. Nothing ever read the column
-- back into a request either, so there is nothing to migrate out of it.
ALTER TABLE app_chat_messages DROP COLUMN IF EXISTS thought_signature;
ALTER TABLE app_chat_messages ADD COLUMN IF NOT EXISTS reasoning_details jsonb;
