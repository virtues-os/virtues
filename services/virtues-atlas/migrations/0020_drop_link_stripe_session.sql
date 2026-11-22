-- Drop `device_link.stripe_session_id`.
--
-- It tied a link to the Stripe Checkout session started from that link's
-- web page, so the success page could mint the box's key. That checkout
-- chose the account from the email typed at Stripe, which nobody verified,
-- and it is gone: a link now only signs in, and paying is a separate
-- checkout bound to the account (`metadata[account_id]`). Nothing reads or
-- writes this column, including the image a rollback would start.
ALTER TABLE device_link DROP COLUMN IF EXISTS stripe_session_id;
