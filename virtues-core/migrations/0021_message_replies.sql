-- 0021_message_replies
--
-- A reply the box has drafted for a message thread and is holding for the
-- owner to send, edit, or dismiss. One row per (thread, ask): the message that
-- asked something is the natural key, so re-running the drafter over the same
-- inbound batch cannot pile up duplicates.
--
-- `app_` because this is product state, not an observation: the record holds
-- the messages, this holds what the box proposes to do about one of them.
-- `thread_id` is the chat GUID the collector already ingests on every message
-- (`data_communication_message.thread_id`), and it is also exactly the id
-- macOS Messages accepts when told to send — so the row carries everything a
-- device needs to act, with no second lookup.
--
-- Status is a text CHECK rather than an enum: the model reads this schema at
-- runtime and a literal list is what it matches. `sent` is the draft going
-- out (edited or not, from the Mac or typed by hand); `answered` is the owner
-- replying in their own words instead. `sent_text` is what actually went out
-- in either case, which is what lets the drafter be measured against what
-- people really send.
CREATE TABLE IF NOT EXISTS app_message_replies (
    id text PRIMARY KEY,
    thread_id text NOT NULL,
    ask_stream_id text NOT NULL,
    ask_text text NOT NULL,
    from_handle text NOT NULL,
    from_name text,
    is_group boolean NOT NULL DEFAULT false,
    draft text NOT NULL,
    rationale text,
    trigger text NOT NULL DEFAULT 'auto' CHECK (trigger IN ('auto', 'manual')),
    status text NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'sent', 'answered', 'dismissed', 'expired')),
    sent_text text,
    sent_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    UNIQUE (thread_id, ask_stream_id)
);

-- The device polls for pending rows, newest first; everything else is by id.
CREATE INDEX IF NOT EXISTS app_message_replies_pending_idx
    ON app_message_replies (created_at DESC)
    WHERE status = 'pending';
