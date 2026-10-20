-- 0055_add_message_outbox
--
-- Messages the assistant sends as the owner, one row per message. A send
-- waits here before it goes: five seconds by default, so the owner can cancel
-- it from the chat, or until the time they asked for. Then the Mac hands it to
-- Messages, and the row follows it until chat.db says it was delivered or not.
--
-- `thread_id` is the chat.db GUID as chat.db wrote it, or NULL when the owner
-- has no thread with them yet and the Mac sends to `handle`. `message_id`
-- links the row to the owner's own message once the collector syncs it back.

CREATE TABLE app_message_outbox (
    id           text PRIMARY KEY,
    chat_id      text,
    person_id    text,
    recipient    text NOT NULL,
    handle       text NOT NULL,
    thread_id    text,
    body         text NOT NULL,
    send_at      timestamptz NOT NULL,
    state        text NOT NULL DEFAULT 'pending'
                 CHECK (state IN ('pending', 'sending', 'sent', 'delivered', 'failed', 'cancelled')),
    error        text,
    sent_at      timestamptz,
    delivered_at timestamptz,
    message_id   text,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

-- The worker's two questions: what is due, and what is waiting on chat.db.
CREATE INDEX idx_app_message_outbox_open ON app_message_outbox (send_at)
    WHERE state IN ('pending', 'sending', 'sent');
