-- Migration: 013_message_idempotency
-- Purpose: Add idempotency support for message IDs to prevent duplicate submissions

--------------------------------------------------------------------------------
-- IDEMPOTENCY INDEX
-- Ensures unique message IDs per session (allows same ID in different sessions)
--------------------------------------------------------------------------------

-- Note: The id column is already the PRIMARY KEY in app_chat_messages,
-- which provides global uniqueness. For client-generated IDs, we use
-- INSERT OR IGNORE in the application layer to handle duplicates gracefully.
-- No additional schema change needed - the primary key constraint is sufficient.
