-- The banks each account has linked through Plaid, so the subscription can
-- cap them. Plaid bills a monthly fee per linked bank until the bank is
-- removed, so a linked bank is the cost a subscription carries, not a call.
--
-- This is current state, not a log: a row is added when a bank is linked and
-- deleted when it is removed, and nothing records when or how the bank is
-- used. `item_hash` is sha256 of Plaid's opaque item id, so the table names no
-- bank and holds nothing Plaid's API accepts.
CREATE TABLE IF NOT EXISTS linked_banks (
    account_id text NOT NULL,
    item_hash bytea NOT NULL,
    PRIMARY KEY (account_id, item_hash)
);
