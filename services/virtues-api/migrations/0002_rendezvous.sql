-- Blind rendezvous: publish_id -> opaque ciphertext.
--
-- The only Virtues-cloud touchpoint in the otherwise-direct WireGuard remote
-- access path. It lets a paired phone relearn the home box's current public
-- endpoint after the ISP rotates the prefix — without this service ever being
-- able to read that endpoint or tie it to anyone.
--
-- The blob is the box's WG endpoint, encrypted under a per-box key K that lives
-- ONLY on the box + its paired devices. virtues-api never holds K, so the
-- stored value is meaningless here. `publish_id` is an opaque, unguessable
-- capability (128-bit random) — it is NOT a customer or bearer identifier.
--
-- Privacy invariant (Lint-10): NO customer column, NO bearer column, NO join
-- key of any kind. The bearer that authorizes a PUT is verified and discarded
-- by the handler — never written beside the publish_id. A dump of this table
-- yields only opaque ciphertext keyed by random strings.
--
-- See Virtues-API.md (the four parties) and
-- docs/wireguard-pairing.md §6 (the decision).
CREATE TABLE rendezvous (
    -- Opaque base64url capability minted on the box at pairing. PK + read cap.
    publish_id   text PRIMARY KEY,

    -- AES-256-GCM(nonce || ciphertext || tag) of the endpoint blob, under K.
    ciphertext   bytea NOT NULL,

    -- Last publish. Refreshed on every PUT.
    updated_at   timestamptz NOT NULL DEFAULT now(),

    -- TTL'd. Refreshed on each PUT, so a live box never expires; a dead one
    -- ages out and the hourly sweeper reaps it. No privacy weight — nothing
    -- legible to delete.
    expires_at   timestamptz NOT NULL
);

CREATE INDEX rendezvous_expires_at_idx ON rendezvous (expires_at);
