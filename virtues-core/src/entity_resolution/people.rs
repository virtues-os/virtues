//! People Resolution
//!
//! Resolves people from multiple sources to canonical wiki_people entities.
//!
//! ## Sources
//!
//! 1. **Calendar attendees**: an address that shares a meeting with you.
//! 2. **Email you sent**: everyone you wrote to.
//! 3. **Email you received**: from someone you already know, or have written to.
//! 4. **Messages**, both directions, by handle.
//!
//! ## Who becomes a person
//!
//! Someone you have had two-way contact with: on a contact card, in a meeting
//! with you, or written to by you. A one-way sender (a newsletter, a receipt,
//! a stranger's cold email) gets no person and no ref. Minting one per unseen
//! sender filled a real box with 257 "people", half of them no-reply robots,
//! none of whom you had ever written to. An address with automated signals
//! (`is_automated_address`) never becomes a person at all.

use std::collections::HashSet;

use uuid::Uuid;

use super::TimeWindow;
use crate::database::Database;
use crate::error::Result;
use crate::ids;

/// Resolve people from all sources in time window
///
/// Returns the total number of people resolved.
pub async fn resolve_people(db: &Database, window: TimeWindow) -> Result<usize> {
    tracing::info!(
        start = %window.start,
        end = %window.end,
        "Resolving people from all sources"
    );

    // First, so every pass below matches against complete handles.
    sync_person_handles(db).await?;
    let owner = owner_addresses(db).await?;

    let mut total_resolved = 0;

    // 1. Resolve from calendar attendees
    total_resolved += resolve_calendar_attendees(db, window, &owner).await?;

    // 2. Everyone you wrote to. Before the senders, so a reply you sent today
    //    makes yesterday's email from them resolvable.
    total_resolved += resolve_email_recipients(db, window, &owner).await?;

    // 3. Resolve from email senders
    total_resolved += resolve_email_senders(db, window).await?;

    // 4 & 5. The two message passes. Deliberately NOT window-bounded — see
    //    `resolve_message_senders` — which is also why they need the gate below.
    if message_work_is_possible(db).await? {
        // 4. Message senders.
        total_resolved += resolve_message_senders(db).await?;

        // 5. The *recipient* of the messages you sent. Same anti-join shape as
        //    (4), same reason it takes no window.
        total_resolved += resolve_message_recipients(db).await?;
    }

    tracing::info!(
        people_resolved = total_resolved,
        "People resolution completed"
    );

    Ok(total_resolved)
}

/// Give every person the normal form of every address and number they hold.
///
/// `handles` is what every resolver matches on, and it used to be written in
/// exactly one place: when a contact card *created* a person. A card merged
/// into an existing person, a number added on the page, a person minted from
/// an email, and two people merged all kept whatever handles they had, so
/// messages from those numbers and addresses never reached them.
///
/// So it is derived here, from `emails` and `phones`, for everyone, every
/// tick. Additive only: a handle already held is never taken away, because
/// losing one silently stops resolution, and the cost of keeping a stale one
/// is a number that still finds the person it used to belong to.
async fn sync_person_handles(db: &Database) -> Result<u64> {
    let rows: Vec<(String, serde_json::Value, serde_json::Value, serde_json::Value)> =
        sqlx::query_as("SELECT id, emails, phones, handles FROM wiki_people")
            .fetch_all(db.pool())
            .await?;
    let strings = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            // absent-ok: a column that is not a JSON array holds no addresses
            .unwrap_or_default()
    };
    let mut ids = Vec::new();
    let mut handles = Vec::new();
    for (id, emails, phones, have) in &rows {
        let have = strings(have);
        let emails = strings(emails);
        let phones = strings(phones);
        let derived = virtues_helpers::handles::normalized_handles(
            emails.iter().map(String::as_str),
            phones.iter().map(String::as_str),
        );
        let want = union_handles(&have, &derived);
        if want.len() != have.len() {
            ids.push(id.clone());
            handles.push(serde_json::json!(want).to_string());
        }
    }
    if ids.is_empty() {
        return Ok(0);
    }
    let n = sqlx::query(
        "UPDATE wiki_people p SET handles = u.h::jsonb, updated_at = now() \
         FROM unnest($1::text[], $2::text[]) AS u(id, h) WHERE p.id = u.id",
    )
    .bind(&ids)
    .bind(&handles)
    .execute(db.pool())
    .await?
    .rows_affected();
    tracing::info!(people = n, "gave people the handles their addresses and numbers imply");
    Ok(n)
}

/// `have` plus whatever `derived` adds, sorted and without repeats.
fn union_handles(have: &[String], derived: &[String]) -> Vec<String> {
    let mut out: Vec<String> = have.iter().chain(derived).cloned().collect();
    out.sort();
    out.dedup();
    out
}

/// The addresses that are you: every address you have sent email from, and
/// the handles on your own person. Never a person of their own, never a ref
/// to anyone: you are not an attendee of your own calendar or the recipient
/// of your own cc.
async fn owner_addresses(db: &Database) -> Result<HashSet<String>> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT lower(from_email) FROM data_communication_email \
         WHERE direction = 'sent' AND from_email <> '' \
         UNION \
         SELECT jsonb_array_elements_text(p.handles) FROM wiki_people p \
         JOIN app_user_profile u ON u.self_person_id = p.id",
    )
    .fetch_all(db.pool())
    .await?;
    Ok(rows.into_iter().collect())
}

/// Does this address belong to a machine rather than a person?
///
/// Narrow on purpose, and only ever used to *refuse* a new person: a false
/// positive costs one person you can still add by hand, a false negative a
/// robot on your People list.
fn is_automated_address(addr: &str) -> bool {
    let Some((local, domain)) = addr.rsplit_once('@') else {
        return true;
    };
    let local = local.split('+').next().unwrap_or(local).to_ascii_lowercase();
    let domain = domain.to_ascii_lowercase();
    // Rooms, resource calendars and group calendars are attendees, not people.
    if domain.ends_with("calendar.google.com") {
        return true;
    }
    let squashed: String = local.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    const PREFIXES: [&str; 3] = ["noreply", "donotreply", "mailerdaemon"];
    if PREFIXES.iter().any(|p| squashed.starts_with(p)) {
        return true;
    }
    const EXACT: [&str; 22] = [
        "notifications", "notification", "notify", "alerts", "alert", "billing",
        "info", "news", "newsletter", "newsletters", "updates", "postmaster",
        "bounce", "bounces", "receipts", "receipt", "marketing", "support",
        "invoice", "invoices", "calendar", "reply",
    ];
    EXACT.contains(&squashed.as_str())
}

/// Who an address belongs to, if anyone.
enum Holder {
    /// No person holds it.
    Nobody,
    /// Exactly one person.
    One(String),
    /// More than one person holds it, so it belongs to none of them. Same rule
    /// as a shared phone number in `resolve_message_senders`: guessing which of
    /// two cards an email came from is worse than leaving it, and the person
    /// page offers "Same as…?" for exactly this.
    Several,
}

async fn holder_of(db: &Database, addr: &str) -> Result<Holder> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM wiki_people WHERE handles ? $1 LIMIT 2")
        .bind(addr)
        .fetch_all(db.pool())
        .await?;
    Ok(match ids.as_slice() {
        [] => Holder::Nobody,
        [one] => Holder::One(one.clone()),
        _ => Holder::Several,
    })
}

/// Have you ever written to this address?
async fn owner_wrote_to(db: &Database, addr: &str) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM data_communication_email e \
         CROSS JOIN LATERAL jsonb_array_elements_text(e.to_emails || e.cc_emails || e.bcc_emails) AS t(a) \
         WHERE e.direction = 'sent' AND lower(t.a) = $1)",
    )
    .bind(addr)
    .fetch_one(db.pool())
    .await?)
}

/// A new person for an address you have had two-way contact with.
///
/// The id is a hash of the address, so creating the same person twice is a
/// no-op. `handles` is written here too, so the person is resolvable on the
/// next message without waiting for `sync_person_handles`.
async fn create_person_for_address(db: &Database, addr: &str, display_name: Option<&str>) -> Result<String> {
    let name = display_name
        .map(str::trim)
        .filter(|n| !n.is_empty() && !n.contains('@'))
        .map(str::to_string)
        .unwrap_or_else(|| extract_name_from_email(addr));
    let person_id = ids::generate_id(ids::WIKI_PERSON_PREFIX, &[addr]);
    sqlx::query(
        "INSERT INTO wiki_people (id, name, emails, handles) VALUES ($1, $2, $3, $3) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(&person_id)
    .bind(&name)
    .bind(serde_json::json!([addr]))
    .execute(db.pool())
    .await?;
    tracing::info!(person_id = %person_id, name = %name, "created a person you have written to or met");
    Ok(person_id)
}

/// What the two message passes read, reduced to four numbers.
///
/// Not a cursor and not a window — a fingerprint of the *inputs*, so the gate
/// below keeps every property the passes were designed for while skipping the
/// work when nothing they read has moved.
///
/// - message count: a message arriving or being deleted is new work.
/// - people count and the newest `wiki_people.updated_at`: a contact added, or
///   an existing contact gaining a phone number (the contacts upsert stamps
///   `updated_at = now()`), makes old messages resolvable. A count alone would
///   miss the edit, which is the case the sender pass's own doc comment
///   promises: "on the day you save that contact it becomes work again".
/// - message-ref count: if refs are deleted, they must be rebuilt. The same doc
///   comment promises that too, and a watermark over our own writes would not
///   notice a deletion; a count does.
#[derive(Debug, Clone, PartialEq)]
struct MessageWorkFingerprint {
    messages: i64,
    people: i64,
    people_touched_at: Option<chrono::DateTime<chrono::Utc>>,
    message_refs: i64,
}

/// The last fingerprint that came back with nothing to do. Process-local on
/// purpose: a restart simply costs one full reconciliation, which is the
/// behavior we had all the time, and it needs no table, no migration and no
/// state that can disagree with the database.
static LAST_IDLE_FINGERPRINT: std::sync::Mutex<Option<MessageWorkFingerprint>> =
    std::sync::Mutex::new(None);

/// Should the message passes run at all?
///
/// They are full reconciliations — "which resolvable message has no ref yet?" —
/// and that is the right *semantics*: no cursor to rewind, no window for a
/// twenty-year backfill to fall outside of, self-healing if refs are deleted.
/// What it is not is the right *frequency*. Every fifteen minutes the sender
/// query re-derived all ~96k already-resolved messages and anti-joined each one
/// away to return zero rows, at 1.0s measured on a real box; the recipient pass
/// does the same over threads. 623 of one week's 743 slow-statement warnings
/// were these two, and the cost grows with the number of messages already DONE,
/// which is exactly backwards.
///
/// So: keep the reconciliation, gate the tick. Four counts (~75ms) decide
/// whether anything the passes read has changed since the last pass that found
/// nothing. If not, there is provably nothing to do — every way work can appear
/// moves one of them.
///
/// `normalize_pending_handles` runs FIRST and reports what it wrote, because a
/// newly normalized handle is new work that changes none of the four (it
/// rewrites `from_handle` in place). When it did anything, the gate is skipped
/// rather than consulted.
async fn message_work_is_possible(db: &Database) -> Result<bool> {
    let normalized = normalize_pending_handles(db).await?;

    let row = sqlx::query!(
        r#"
        SELECT (SELECT count(*) FROM data_communication_message)     AS "messages!",
               (SELECT count(*) FROM wiki_people)                    AS "people!",
               (SELECT max(updated_at) FROM wiki_people)             AS people_touched_at,
               (SELECT count(*) FROM wiki_refs
                 WHERE source_table = 'data_communication_message')  AS "message_refs!"
        "#
    )
    .fetch_one(db.pool())
    .await?;

    let now = MessageWorkFingerprint {
        messages: row.messages,
        people: row.people,
        people_touched_at: row.people_touched_at,
        message_refs: row.message_refs,
    };

    let mut last = LAST_IDLE_FINGERPRINT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !work_is_possible(last.as_ref(), &now, normalized) {
        tracing::debug!("message resolution: inputs unchanged, skipping the reconciliation");
        return Ok(false);
    }
    // Recorded BEFORE the passes run. If they write refs, the count moves and the
    // next tick reconciles once more, finds nothing, and records the settled
    // fingerprint — one extra pass after real work, never a missed one.
    *last = Some(now);
    Ok(true)
}

/// The decision itself, with no database and no global in it.
///
/// Split out because the rule that matters — *skip only when we have seen this
/// exact state before and normalization wrote nothing* — is the part that must
/// never quietly drift to "skip". A gate that wrongly opens costs a second; a
/// gate that wrongly closes means a message never gets a name, and nothing
/// reports it.
fn work_is_possible(
    last: Option<&MessageWorkFingerprint>,
    now: &MessageWorkFingerprint,
    normalized: u64,
) -> bool {
    normalized > 0 || last != Some(now)
}

/// How many rows a single pass of the message resolver claims at a time. Both loops
/// below drain — they keep going while batches come back full — so this bounds
/// memory and statement size, not total throughput.
const MESSAGE_BATCH: i64 = 5_000;

/// Turn `+15125550100` into Nick.
///
/// Email senders resolved. Calendar attendees resolved. Message senders never did —
/// there was no resolver for `data_communication_message` at all. So the box knew
/// 525 people, held thousands of messages, and connected none of them: every message
/// said a phone number and not one said a name.
///
/// That is the difference between a log and a memory. "Sucks you're going to have to
/// cancel — +15125550100" is a line of data. The same sentence with "Nick" on it is a
/// life, and it is what makes the rest of the system worth querying.
///
/// # Why this one takes no `TimeWindow`
///
/// Every other resolver here is driven by a rolling window over the *event's*
/// timestamp — "re-resolve the last 30 hours". That works for GPS, where a point is
/// collected moments after it happens, so event time and arrival time agree.
///
/// It is exactly wrong for messages. The chat.db backfill walks twenty years of
/// history, so a message that lands *right now* carries `timestamp = 2018-04-11` and
/// falls outside a window anchored to `now()` — permanently. A window asks "what
/// happened recently?" when the question that needs answering is "what have I not
/// done yet?", and those two agree only when data arrives in the order it occurred.
///
/// So this is driven by the *absence of its own output*: a message is work if it has
/// no sender ref yet. No cursor to rewind, no window to fall outside of, nothing to
/// keep in sync. Delete the refs and they are rebuilt; crash mid-pass and the next
/// tick resumes exactly where it stopped; land a message from 2018 and it is picked
/// up on the next tick like any other. The same shape `search::indexer` already uses
/// (`LEFT JOIN search_embeddings … WHERE se.id IS NULL`).
///
/// # The trap in that, and the way out
///
/// A naive "no ref yet" anti-join never terminates here. Unlike email — which mints a
/// person for any unknown address, so a ref always appears — an unknown *number* gets
/// no person, deliberately (below). So the ~1,200 numbers with no contact would have
/// no ref, forever, and would be re-examined every fifteen minutes until the heat
/// death of the box.
///
/// The fix falls out of the data model rather than being bolted onto it: **inner-join
/// the known handles**. A message is only work if its sender is someone we could
/// resolve *and* haven't. An unknown number matches no person, so it is not work and
/// costs nothing — and on the day you save that contact it becomes work again, on its
/// own. Self-healing in both directions, with no bookkeeping.
///
/// # Why an unknown number does not become a person
///
/// An unrecognized email address at least carries a display name. A bare phone number
/// carries nothing, so minting a person per unknown number would fill the graph with
/// hundreds of ghosts called "+18005550199". They stay unresolved until a contact
/// turns up — the honest state.
async fn resolve_message_senders(db: &Database) -> Result<usize> {
    // `normalize_pending_handles` used to run here. It now runs in
    // `message_work_is_possible`, which must see its result to decide — and
    // running it twice would be a wasted pass over the pending index.
    let mut resolved = 0usize;
    loop {
        // One handle must mean one person. If two contacts claim the same number the
        // contact data is wrong, and guessing which of them said something is worse
        // than admitting we don't know — so an ambiguous handle owns nobody and its
        // messages simply stay unresolved.
        let rows = sqlx::query!(
            r#"
            WITH handle_owner AS (
                SELECT h.handle,
                       min(p.id)             AS person_id,
                       min(p.name) AS name
                FROM wiki_people p
                CROSS JOIN LATERAL jsonb_array_elements_text(p.handles) AS h(handle)
                GROUP BY h.handle
                HAVING count(DISTINCT p.id) = 1
            )
            SELECT m.id            AS "msg_id!",
                   o.person_id     AS "person_id!",
                   o.name AS "name!"
            FROM data_communication_message m
            JOIN handle_owner o ON o.handle = m.from_handle
            LEFT JOIN wiki_refs r
                   ON r.source_table = 'data_communication_message'
                  AND r.source_id = m.id
                  AND r.role = 'sender'
            WHERE m.from_handle <> ''
              AND r.id IS NULL
            LIMIT $1
            "#,
            MESSAGE_BATCH
        )
        .fetch_all(db.pool())
        .await?;

        if rows.is_empty() {
            break;
        }
        let batch = rows.len();

        let msg_ids: Vec<String> = rows.iter().map(|r| r.msg_id.clone()).collect();
        let person_ids: Vec<String> = rows.iter().map(|r| r.person_id.clone()).collect();
        let names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
        let ref_ids: Vec<String> = rows
            .iter()
            .map(|r| ids::generate_id("eref", &[&r.msg_id, &r.person_id, "sender"]))
            .collect();

        // The ref is the real artifact: it is what lets you traverse from a message to
        // everything else about the person who sent it.
        sqlx::query!(
            r#"
            INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at)
            SELECT u.ref_id, 'person', u.person_id, 'data_communication_message', u.msg_id, 'sender', m.occurred_at
            FROM UNNEST($1::text[], $2::text[], $3::text[]) AS u(ref_id, person_id, msg_id)
            JOIN data_communication_message m ON m.id = u.msg_id
            ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING
            "#,
            &ref_ids,
            &person_ids,
            &msg_ids
        )
        .execute(db.pool())
        .await?;

        // `from_name` is the convenience copy — what a day summary or a chat prompt can
        // read without a join. Only fill it in where it's empty; a name a human set by
        // hand outranks one we inferred.
        sqlx::query!(
            r#"
            UPDATE data_communication_message m
            SET from_name = u.name
            FROM UNNEST($1::text[], $2::text[]) AS u(id, name)
            WHERE m.id = u.id AND m.from_name IS NULL
            "#,
            &msg_ids,
            &names
        )
        .execute(db.pool())
        .await?;

        resolved += batch;
        if (batch as i64) < MESSAGE_BATCH {
            break;
        }
    }

    if resolved > 0 {
        tracing::info!(resolved, "Message senders resolved to people");
    }
    Ok(resolved)
}

/// Link the messages *you* sent to the person you sent them to.
///
/// This is the other half of a conversation. The sender pass (above) resolves every
/// *inbound* message — "+15125550137" → Nick — but says nothing about your replies,
/// and so "did we ever text Nick?" returned only their side: their three messages, none
/// of yours. The reason is structural, not a bug in the join. A message you sent has
/// `is_from_me = true`, which the transform records as `from_identifier = "me"` and
/// `from_handle = ""` — because the chat.db `handle` names the *other* party even on
/// your own rows, so trusting it would attribute your message to its recipient. An
/// empty handle matches no person, so a self-authored row gets no ref at all and is
/// invisible to every person-scoped query. There is no owner entity to point it at,
/// and inventing one would not help: the useful link is not "me", it is *who I was
/// talking to*.
///
/// That party is recoverable without any new column. `thread_id` groups a
/// conversation, and the counterparty is simply the one known person among the
/// thread's *inbound* handles — which the sender side has already resolved. So each
/// sent message gets a `recipient` ref to that person, mirroring how
/// `data_communication_email` resolves both sender and recipient. A person query then
/// asks for `role IN ('sender','recipient')` and gets **both** halves of the thread,
/// direction-blind.
///
/// # Scope, and why the anti-join still terminates
///
/// Only unambiguous **1:1** threads (`is_group_message = false`, exactly one distinct
/// known counterparty). A group's "recipient" is many people — a later `participant`
/// pass, not this one — so group and unknown-counterparty threads simply produce no
/// `thread_party` row, are never selected, and cost nothing. That is what keeps the
/// `r.id IS NULL` drain from spinning on rows it can never resolve: exactly the same
/// self-healing shape as the sender pass, which is only *work* for a handle we could
/// resolve *and* haven't. Save the contact and the thread becomes resolvable on its
/// own; until then it stays honestly unlinked.
async fn resolve_message_recipients(db: &Database) -> Result<usize> {
    let mut resolved = 0usize;
    loop {
        let rows = sqlx::query!(
            r#"
            WITH handle_owner AS (
                SELECT h.handle,
                       min(p.id)             AS person_id,
                       min(p.name) AS name
                FROM wiki_people p
                CROSS JOIN LATERAL jsonb_array_elements_text(p.handles) AS h(handle)
                GROUP BY h.handle
                HAVING count(DISTINCT p.id) = 1
            ),
            -- The counterparty of each 1:1 thread: the single distinct known person
            -- among its inbound handles. `HAVING count(DISTINCT …) = 1` drops any
            -- thread whose inbound side resolves to two different people (bad contact
            -- data) — guessing the recipient is worse than leaving it unresolved.
            thread_party AS (
                SELECT m.thread_id,
                       min(o.person_id)      AS person_id,
                       min(o.name) AS name
                FROM data_communication_message m
                JOIN handle_owner o ON o.handle = m.from_handle
                WHERE m.from_handle <> ''
                  AND m.is_group_message = FALSE
                  AND m.thread_id IS NOT NULL
                GROUP BY m.thread_id
                HAVING count(DISTINCT o.person_id) = 1
            )
            SELECT m.id             AS "msg_id!",
                   tp.person_id     AS "person_id!",
                   tp.name AS "name!"
            FROM data_communication_message m
            JOIN thread_party tp ON tp.thread_id = m.thread_id
            LEFT JOIN wiki_refs r
                   ON r.source_table = 'data_communication_message'
                  AND r.source_id = m.id
                  AND r.role = 'recipient'
            WHERE (m.metadata->>'is_from_me')::boolean IS TRUE
              AND r.id IS NULL
            LIMIT $1
            "#,
            MESSAGE_BATCH
        )
        .fetch_all(db.pool())
        .await?;

        if rows.is_empty() {
            break;
        }
        let batch = rows.len();

        let msg_ids: Vec<String> = rows.iter().map(|r| r.msg_id.clone()).collect();
        let person_ids: Vec<String> = rows.iter().map(|r| r.person_id.clone()).collect();
        let ref_ids: Vec<String> = rows
            .iter()
            .map(|r| ids::generate_id("eref", &[&r.msg_id, &r.person_id, "recipient"]))
            .collect();

        sqlx::query!(
            r#"
            INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at)
            SELECT u.ref_id, 'person', u.person_id, 'data_communication_message', u.msg_id, 'recipient', m.occurred_at
            FROM UNNEST($1::text[], $2::text[], $3::text[]) AS u(ref_id, person_id, msg_id)
            JOIN data_communication_message m ON m.id = u.msg_id
            ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING
            "#,
            &ref_ids,
            &person_ids,
            &msg_ids
        )
        .execute(db.pool())
        .await?;

        // `from_name` is deliberately NOT filled here. It names who *sent* the
        // message — that is you — so writing the recipient's name onto your own row
        // would render "Nick: <your reply>". The link lives in the ref; the plain
        // column stays honest.

        resolved += batch;
        if (batch as i64) < MESSAGE_BATCH {
            break;
        }
    }

    if resolved > 0 {
        tracing::info!(resolved, "Sent messages resolved to recipients");
    }
    Ok(resolved)
}

/// Fill `from_handle` for messages that predate it (or that some future collector
/// forgets to set). The transform normalizes at write time, so in the steady state
/// this drains to nothing on the first tick and costs an empty index scan thereafter.
///
/// Normalization stays in Rust — one implementation, in `virtues_helpers::handles`,
/// shared with the transform and with contact ingest. A second copy of the E.164 rules
/// written in SQL would drift, and the two halves of a join silently disagreeing about
/// what a phone number *is* was the original bug.
///
/// Returns how many rows it filled in. `message_work_is_possible` needs that
/// number: a freshly normalized handle is new work for the sender pass, and it
/// changes none of the counts that gate reads.
async fn normalize_pending_handles(db: &Database) -> Result<u64> {
    let mut normalized = 0u64;
    loop {
        let rows = sqlx::query!(
            r#"
            SELECT id, from_identifier
            FROM data_communication_message
            WHERE from_handle IS NULL
            LIMIT $1
            "#,
            MESSAGE_BATCH
        )
        .fetch_all(db.pool())
        .await?;

        if rows.is_empty() {
            return Ok(normalized);
        }
        let batch = rows.len();

        let mut ids: Vec<String> = Vec::with_capacity(batch);
        let mut handles: Vec<String> = Vec::with_capacity(batch);
        for row in rows {
            ids.push(row.id);
            // `None` — a short code, or ourselves — is stored as '', which means
            // "asked, and the answer is nobody". That is what keeps it out of the work
            // queue for good; leaving it NULL would re-offer it forever.
            handles.push(
                virtues_helpers::handles::normalize_handle(&row.from_identifier)
                    .unwrap_or_default(),
            );
        }

        let updated = sqlx::query!(
            r#"
            UPDATE data_communication_message m
            SET from_handle = u.handle
            FROM UNNEST($1::text[], $2::text[]) AS u(id, handle)
            WHERE m.id = u.id
            "#,
            &ids,
            &handles
        )
        .execute(db.pool())
        .await?
        .rows_affected();

        // Claimed rows but changed none: the loop would spin on the same batch forever.
        // Bail loudly rather than pin a core for the life of the daemon.
        if updated == 0 {
            tracing::error!(
                batch,
                "from_handle backfill selected rows but updated none — aborting drain"
            );
            return Ok(normalized);
        }

        normalized += updated;
        if (batch as i64) < MESSAGE_BATCH {
            return Ok(normalized);
        }
    }
}

/// Resolve calendar attendees in the time window. Sharing a meeting with you
/// is two-way contact, so an attendee nobody holds becomes a person, unless
/// the address is you or a machine (a room, a resource calendar).
async fn resolve_calendar_attendees(
    db: &Database,
    window: TimeWindow,
    owner: &HashSet<String>,
) -> Result<usize> {
    let calendar_events = fetch_calendar_events(db, window).await?;

    if calendar_events.is_empty() {
        tracing::debug!("No calendar events to process");
        return Ok(0);
    }

    tracing::debug!(
        event_count = calendar_events.len(),
        "Fetched calendar events for people resolution"
    );

    let mut total_people_resolved = 0;
    for event in calendar_events {
        match resolve_and_link_event_attendees(db, &event, owner).await {
            Ok(count) => total_people_resolved += count,
            Err(e) => {
                tracing::warn!(
                    event_id = %event.id,
                    error = %e,
                    "Failed to resolve attendees for event"
                );
            }
        }
    }

    tracing::debug!(
        people_resolved = total_people_resolved,
        "Calendar attendee resolution completed"
    );

    Ok(total_people_resolved)
}

/// Link email you received to the person who sent it.
///
/// Only email you received: the sender of email you sent is you, and filing it
/// as a ref made your own address a person. And only from someone you already
/// know or have written to; see the module doc.
async fn resolve_email_senders(db: &Database, window: TimeWindow) -> Result<usize> {
    let emails = fetch_unresolved_emails(db, window).await?;

    if emails.is_empty() {
        tracing::debug!("No emails to process for sender resolution");
        return Ok(0);
    }

    tracing::debug!(
        email_count = emails.len(),
        "Fetched emails for sender resolution"
    );

    let mut total_resolved = 0;
    for email_record in emails {
        match resolve_and_link_email_sender(db, &email_record).await {
            Ok(true) => total_resolved += 1,
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(
                    email_id = %email_record.id,
                    from_email = %email_record.from_email,
                    error = %e,
                    "Failed to resolve sender for email"
                );
            }
        }
    }

    tracing::debug!(
        people_resolved = total_resolved,
        "Email sender resolution completed"
    );

    Ok(total_resolved)
}

/// Email record for sender resolution
#[derive(Debug)]
struct EmailRecord {
    id: String,
    from_email: String,
    from_name: Option<String>,
}

/// Received email in the window with no sender ref yet.
async fn fetch_unresolved_emails(db: &Database, window: TimeWindow) -> Result<Vec<EmailRecord>> {
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT e.id, e.from_email, e.from_name
        FROM data_communication_email e
        WHERE e.occurred_at >= $1
          AND e.occurred_at < $2
          AND e.direction = 'received'
          AND e.from_email <> ''
          AND NOT EXISTS (
              SELECT 1 FROM wiki_refs er
              WHERE er.source_table = 'data_communication_email'
                AND er.source_id = e.id
                AND er.role = 'sender'
          )
        ORDER BY e.occurred_at ASC
        LIMIT 1000
        "#,
    )
    .bind(window.start)
    .bind(window.end)
    .fetch_all(db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, from_email, from_name)| EmailRecord { id, from_email, from_name })
        .collect())
}

/// Link one received email to its sender. Returns whether it was linked.
async fn resolve_and_link_email_sender(db: &Database, email_record: &EmailRecord) -> Result<bool> {
    let addr = email_record.from_email.trim().to_lowercase();

    let person_id = match holder_of(db, &addr).await? {
        Holder::One(id) => {
            name_from_header(db, &id, email_record.from_name.as_deref()).await?;
            id
        }
        Holder::Several => return Ok(false),
        Holder::Nobody => {
            if is_automated_address(&addr) || !owner_wrote_to(db, &addr).await? {
                return Ok(false);
            }
            create_person_for_address(db, &addr, email_record.from_name.as_deref()).await?
        }
    };

    let ref_id = ids::generate_id("eref", &[&email_record.id, &person_id, "sender"]);
    sqlx::query(
        r#"
        INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at)
        SELECT $1, 'person', $2, 'data_communication_email', $3, 'sender', occurred_at
        FROM data_communication_email WHERE id = $3
        ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING
        "#,
    )
    .bind(ref_id)
    .bind(&person_id)
    .bind(&email_record.id)
    .execute(db.pool())
    .await?;

    Ok(true)
}

/// A name from an email header replaces one the record made up from an
/// address ("jdoe"), and nothing else: never a name from a contact card,
/// never one you gave.
async fn name_from_header(db: &Database, person_id: &str, display_name: Option<&str>) -> Result<()> {
    let Some(name) = display_name.map(str::trim).filter(|n| n.contains(' ') && !n.contains('@')) else {
        return Ok(());
    };
    sqlx::query(
        "UPDATE wiki_people SET name = $1, updated_at = now() \
         WHERE id = $2 AND position(' ' in name) = 0 \
           AND metadata->>'source' IS NULL AND NOT metadata ? 'name_edited_at'",
    )
    .bind(name)
    .bind(person_id)
    .execute(db.pool())
    .await?;
    Ok(())
}

/// Link email you sent to everyone you sent it to.
///
/// Writing to someone is two-way contact, so a recipient nobody holds becomes
/// a person here, and the email they sent you before you answered is linked to
/// them in the same step: it arrived when they were a one-way sender, and the
/// sender pass only looks back a window.
async fn resolve_email_recipients(
    db: &Database,
    window: TimeWindow,
    owner: &HashSet<String>,
) -> Result<usize> {
    let rows: Vec<(String, serde_json::Value, serde_json::Value, serde_json::Value)> = sqlx::query_as(
        r#"
        SELECT e.id, e.to_emails, e.to_names, e.cc_emails
        FROM data_communication_email e
        WHERE e.occurred_at >= $1
          AND e.occurred_at < $2
          AND e.direction = 'sent'
          AND NOT EXISTS (
              SELECT 1 FROM wiki_refs er
              WHERE er.source_table = 'data_communication_email'
                AND er.source_id = e.id
                AND er.role = 'recipient'
          )
        ORDER BY e.occurred_at ASC
        LIMIT 1000
        "#,
    )
    .bind(window.start)
    .bind(window.end)
    .fetch_all(db.pool())
    .await?;

    // Positional, not filtered: `to_names[i]` names `to_emails[i]`.
    let strings = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            // absent-ok: a non-string entry is an address we cannot use, kept as '' to hold its position
            .map(|a| a.iter().map(|x| x.as_str().unwrap_or_default().to_string()).collect())
            // absent-ok: a column that is not a JSON array holds no addresses
            .unwrap_or_default()
    };
    let mut linked = 0;
    for (email_id, to, to_names, cc) in rows {
        let to = strings(&to);
        let names = strings(&to_names);
        let addressed = to
            .iter()
            .enumerate()
            .map(|(i, a)| (a.clone(), names.get(i).filter(|n| !n.is_empty()).cloned()))
            .chain(strings(&cc).into_iter().map(|a| (a, None)));
        for (raw, name) in addressed {
            let addr = raw.trim().to_lowercase();
            if addr.is_empty() || owner.contains(&addr) || is_automated_address(&addr) {
                continue;
            }
            let person_id = match holder_of(db, &addr).await? {
                Holder::One(id) => id,
                Holder::Several => continue,
                Holder::Nobody => {
                    let id = create_person_for_address(db, &addr, name.as_deref()).await?;
                    link_received_email_from(db, &id, &addr).await?;
                    id
                }
            };
            let ref_id = ids::generate_id("eref", &[&email_id, &person_id, "recipient"]);
            sqlx::query(
                r#"
                INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at)
                SELECT $1, 'person', $2, 'data_communication_email', $3, 'recipient', occurred_at
                FROM data_communication_email WHERE id = $3
                ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING
                "#,
            )
            .bind(ref_id)
            .bind(&person_id)
            .bind(&email_id)
            .execute(db.pool())
            .await?;
            linked += 1;
        }
    }
    Ok(linked)
}

/// Every email you received from `addr`, linked to `person_id` as its sender.
async fn link_received_email_from(db: &Database, person_id: &str, addr: &str) -> Result<()> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM data_communication_email \
         WHERE direction = 'received' AND lower(from_email) = $1",
    )
    .bind(addr)
    .fetch_all(db.pool())
    .await?;
    for email_id in rows {
        let ref_id = ids::generate_id("eref", &[&email_id, person_id, "sender"]);
        sqlx::query(
            r#"
            INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at)
            SELECT $1, 'person', $2, 'data_communication_email', $3, 'sender', occurred_at
            FROM data_communication_email WHERE id = $3
            ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING
            "#,
        )
        .bind(ref_id)
        .bind(person_id)
        .bind(&email_id)
        .execute(db.pool())
        .await?;
    }
    Ok(())
}

/// Calendar event with attendees
#[derive(Debug)]
struct CalendarEvent {
    id: Uuid,
    attendee_identifiers: Vec<String>,
}

/// Fetch calendar events in time window
async fn fetch_calendar_events(db: &Database, window: TimeWindow) -> Result<Vec<CalendarEvent>> {
    let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
        r#"
        SELECT id, attendee_identifiers
        FROM data_calendar_event
        WHERE started_at >= $1
          AND started_at < $2
          AND jsonb_array_length(attendee_identifiers) > 0
        "#,
    )
    .bind(window.start)
    .bind(window.end)
    .fetch_all(db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(id, attendees)| {
            // absent-ok: an attendee list that is not strings names nobody we can resolve
            let identifiers: Vec<String> = serde_json::from_value(attendees).ok()?;
            let id = Uuid::parse_str(&id).ok()?;
            Some(CalendarEvent { id, attendee_identifiers: identifiers })
        })
        .collect())
}

/// Resolve all attendees for an event and link via wiki_refs
///
/// Returns the number of unique people resolved.
async fn resolve_and_link_event_attendees(
    db: &Database,
    event: &CalendarEvent,
    owner: &HashSet<String>,
) -> Result<usize> {
    if event.attendee_identifiers.is_empty() {
        return Ok(0);
    }

    let mut unique_people = HashSet::new();
    let event_id_str = event.id.to_string();

    let timestamp: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT started_at FROM data_calendar_event WHERE id = $1",
    )
    .bind(&event_id_str)
    .fetch_optional(db.pool())
    .await?
    .flatten();

    for raw in &event.attendee_identifiers {
        let addr = raw.trim().to_lowercase();
        if addr.is_empty() || owner.contains(&addr) || is_automated_address(&addr) {
            continue;
        }
        let person_id = match holder_of(db, &addr).await? {
            Holder::One(id) => id,
            Holder::Several => continue,
            Holder::Nobody => create_person_for_address(db, &addr, None).await?,
        };
        if unique_people.insert(person_id.clone()) {
            let ref_id = ids::generate_id("eref", &[&event_id_str, &person_id, "attendee"]);
            sqlx::query(
                r#"
                INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at)
                VALUES ($1, 'person', $2, 'data_calendar_event', $3, 'attendee', $4)
                ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING
                "#,
            )
            .bind(ref_id)
            .bind(&person_id)
            .bind(&event_id_str)
            .bind(timestamp)
            .execute(db.pool())
            .await?;
        }
    }

    Ok(unique_people.len())
}

/// Extract name from email (simple heuristic)
///
/// Examples:
/// - adam.jace@example.com → "Adam Jace"
/// - john.doe@company.co → "John Doe"
/// - user123@domain.com → "user123"
fn extract_name_from_email(email: &str) -> String {
    let local_part = email.split('@').next().unwrap_or(email);

    // Split by dot or underscore
    let parts: Vec<&str> = local_part.split(&['.', '_'][..]).collect();

    if parts.len() > 1 {
        // Capitalize each part
        parts
            .iter()
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        // Just return the local part as-is
        local_part.to_string()
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    fn fp(messages: i64, people: i64, refs: i64) -> MessageWorkFingerprint {
        MessageWorkFingerprint {
            messages,
            people,
            people_touched_at: None,
            message_refs: refs,
        }
    }

    /// Every way work can appear must open the gate. The failure this guards is
    /// silent and permanent in the wrong direction: a message that never gets a
    /// name, with nothing logged, because a fingerprint field stopped moving.
    #[test]
    fn every_way_work_appears_opens_the_gate() {
        let seen = fp(100, 10, 90);

        assert!(!work_is_possible(Some(&seen), &seen, 0), "identical state: skip");
        assert!(work_is_possible(None, &seen, 0), "nothing seen yet: run");
        assert!(work_is_possible(Some(&seen), &fp(101, 10, 90), 0), "a message arrived");
        assert!(work_is_possible(Some(&seen), &fp(99, 10, 90), 0), "a message was deleted");
        assert!(work_is_possible(Some(&seen), &fp(100, 11, 90), 0), "a contact was added");
        assert!(
            work_is_possible(Some(&seen), &fp(100, 10, 89), 0),
            "a ref was deleted — the passes must rebuild it"
        );

        // A contact gaining a phone number changes no count, only the stamp. This
        // is the case the sender pass's doc comment promises out loud.
        let edited = MessageWorkFingerprint {
            people_touched_at: Some(chrono::Utc::now()),
            ..seen.clone()
        };
        assert!(
            work_is_possible(Some(&seen), &edited, 0),
            "an existing contact was edited"
        );

        // And a freshly normalized handle is work that moves nothing at all.
        assert!(
            work_is_possible(Some(&seen), &seen, 1),
            "normalization rewrote from_handle in place"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_name_from_email() {
        assert_eq!(
            extract_name_from_email("adam.jace@example.com"),
            "Adam Jace"
        );
        assert_eq!(extract_name_from_email("john_doe@company.co"), "John Doe");
        assert_eq!(extract_name_from_email("user123@domain.com"), "user123");
        assert_eq!(extract_name_from_email("single@test.com"), "single");
    }

    /// The regression this whole change exists for: a two-sided thread — one message
    /// you received, two you sent — must resolve to the same person on BOTH sides, so
    /// "did we ever text X?" returns your replies and not only theirs.
    ///
    /// Seeds a self-contained thread (unique ids, cleaned up at both ends), runs the
    /// real sender + recipient passes, and asserts:
    ///   1. your sent messages gain a `recipient` ref to the contact (the fix);
    ///   2. a `role IN ('sender','recipient')` query returns all three messages.
    ///
    /// `#[ignore]` by repo convention — needs Postgres:
    ///   DATABASE_URL=postgres://virtues:virtues@localhost:5432/virtues \
    ///     cargo test -p virtues entity_resolution::people -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "needs Postgres (DATABASE_URL)"]
    async fn sent_messages_resolve_to_the_recipient() {
        let pool = virtues_helpers::connect_from_env("recipient-resolution-test")
            .await
            .expect("DATABASE_URL");
        let db = Database::from_pool(pool);

        // Namespaced so the seed can never collide with real rows and always cleans up.
        const P: &str = "test_recipient_pass";
        let person_id = format!("person_{P}");
        let handle = "+15125550137";
        let thread = format!("iMessage;-;{handle};{P}");

        async fn cleanup(db: &Database, person_id: &str, thread: &str) {
            let _ = sqlx::query(
                "DELETE FROM wiki_refs r
                 USING data_communication_message m
                 WHERE r.source_table='data_communication_message'
                   AND r.source_id=m.id AND m.thread_id=$1",
            )
            .bind(thread)
            .execute(db.pool())
            .await;
            let _ = sqlx::query("DELETE FROM data_communication_message WHERE thread_id=$1")
                .bind(thread)
                .execute(db.pool())
                .await;
            let _ = sqlx::query("DELETE FROM wiki_people WHERE id=$1")
                .bind(person_id)
                .execute(db.pool())
                .await;
        }

        // One inbound (she texts you) + two outbound (you reply). Outbound rows carry
        // is_from_me and an EMPTY from_handle — exactly what the transform writes, and
        // exactly why they were invisible before this pass.
        async fn seed_msg(
            db: &Database,
            thread: &str,
            id: &str,
            from_handle: &str,
            from_ident: &str,
            is_from_me: bool,
        ) {
            sqlx::query(
                "INSERT INTO data_communication_message
                   (id, message_id, thread_id, channel, body, from_identifier,
                    from_handle, is_group_message, occurred_at, source_stream_id,
                    source_table, source_provider, metadata)
                 VALUES ($1,$1,$2,'imessage','hi',$3,$4,false, now(), $1,
                         'mac_imessage','mac', $5::jsonb)",
            )
            .bind(id)
            .bind(thread)
            .bind(from_ident)
            .bind(from_handle)
            .bind(serde_json::json!({ "is_from_me": is_from_me }))
            .execute(db.pool())
            .await
            .expect("seed message");
        }

        cleanup(&db, &person_id, &thread).await; // in case a prior run died mid-way

        sqlx::query(
            "INSERT INTO wiki_people (id, name, handles)
             VALUES ($1, 'Nick', $2::jsonb)",
        )
        .bind(&person_id)
        .bind(serde_json::json!([handle]))
        .execute(db.pool())
        .await
        .expect("seed person");

        seed_msg(&db, &thread, &format!("{P}_in1"), handle, handle, false).await;
        seed_msg(&db, &thread, &format!("{P}_out1"), "", "me", true).await;
        seed_msg(&db, &thread, &format!("{P}_out2"), "", "me", true).await;

        // The real passes, in the order resolve_people runs them.
        resolve_message_senders(&db).await.expect("sender pass");
        let n = resolve_message_recipients(&db)
            .await
            .expect("recipient pass");
        assert_eq!(n, 2, "both sent messages should newly resolve to a recipient");

        // 1. Each outbound message now points at the contact via a recipient ref.
        let recipient_refs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM wiki_refs r
             JOIN data_communication_message m ON m.id = r.source_id
             WHERE m.thread_id=$1 AND r.role='recipient' AND r.entity_id=$2
               AND (m.metadata->>'is_from_me')::bool",
        )
        .bind(&thread)
        .bind(&person_id)
        .fetch_one(db.pool())
        .await
        .expect("count recipient refs");
        assert_eq!(recipient_refs, 2, "both your replies link to the contact");

        // 2. The read-side shape "messages with this person" now returns BOTH halves.
        let both_sides: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT m.id)
             FROM data_communication_message m
             JOIN wiki_refs r
               ON r.source_table='data_communication_message' AND r.source_id=m.id
              AND r.entity_type='person' AND r.role IN ('sender','recipient')
             WHERE r.entity_id=$1",
        )
        .bind(&person_id)
        .fetch_one(db.pool())
        .await
        .expect("count both sides");
        assert_eq!(both_sides, 3, "one received + two sent, the whole thread");

        // Idempotent: a second pass is a no-op (the anti-join is satisfied).
        let again = resolve_message_recipients(&db).await.expect("second pass");
        assert_eq!(again, 0, "recipient pass must be self-healing, not re-doing work");

        cleanup(&db, &person_id, &thread).await;
    }
}

#[cfg(test)]
mod two_way_tests {
    use super::*;
    use chrono::{Duration, Utc};
    use sqlx::PgPool;

    fn window() -> TimeWindow {
        TimeWindow::new(Utc::now() - Duration::hours(30), Utc::now() + Duration::hours(1))
    }

    async fn email(pool: &PgPool, id: &str, direction: &str, from: &str, to: &[&str], to_names: &[&str], days_ago: i64) {
        sqlx::query(
            "INSERT INTO data_communication_email \
               (id, message_id, from_email, to_emails, to_names, direction, occurred_at, \
                source_stream_id, source_table, source_provider) \
             VALUES ($1, $1, $2, $3, $4, $5, now() - make_interval(days => $6::int), $1, 'gmail', 'google')",
        )
        .bind(id)
        .bind(from)
        .bind(serde_json::json!(to))
        .bind(serde_json::json!(to_names))
        .bind(direction)
        .bind(days_ago as i32)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn people(pool: &PgPool) -> Vec<(String, serde_json::Value)> {
        sqlx::query_as("SELECT name, handles FROM wiki_people ORDER BY name")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[test]
    fn machines_are_not_people() {
        for a in [
            "no-reply@shop.example.com",
            "noreply+1234@example.com",
            "do_not_reply@example.com",
            "notifications@example.com",
            "info@example.com",
            "c_123abc@resource.calendar.google.com",
            "team_abc@group.calendar.google.com",
            "not-an-address",
        ] {
            assert!(is_automated_address(a), "{a}");
        }
        for a in ["nick@example.com", "david.okafor@example.com", "supporting.cast@example.com"] {
            assert!(!is_automated_address(a), "{a}");
        }
    }

    /// A newsletter, a receipt and a stranger's cold email are not people.
    #[sqlx::test]
    async fn a_one_way_sender_is_not_a_person(pool: PgPool) {
        email(&pool, "e1", "received", "deals@shop.example.com", &["me@example.com"], &[], 0).await;
        email(&pool, "e2", "received", "stranger@example.com", &["me@example.com"], &[], 0).await;
        resolve_people(&Database::from_pool(pool.clone()), window()).await.unwrap();
        assert!(people(&pool).await.is_empty(), "{:?}", people(&pool).await);
    }

    /// Writing to someone makes them a person, links what you sent, and links
    /// what they sent you before you answered, which is outside the window.
    /// Your own address is never a person.
    #[sqlx::test]
    async fn writing_to_someone_makes_them_a_person(pool: PgPool) {
        email(&pool, "old", "received", "Nick@Example.com", &["me@example.com"], &[], 10).await;
        email(&pool, "reply", "sent", "me@example.com", &["Nick@Example.com"], &["Nick Bell"], 0).await;
        resolve_people(&Database::from_pool(pool.clone()), window()).await.unwrap();

        assert_eq!(
            people(&pool).await,
            [("Nick Bell".to_string(), serde_json::json!(["nick@example.com"]))]
        );
        let refs: Vec<(String, String)> =
            sqlx::query_as("SELECT source_id, role FROM wiki_refs ORDER BY source_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(refs, [("old".into(), "sender".into()), ("reply".into(), "recipient".into())]);
    }

    /// A person's handles follow their addresses and numbers, however they got
    /// them, and a handle already held is never dropped.
    #[sqlx::test]
    async fn handles_follow_addresses_and_numbers(pool: PgPool) {
        sqlx::query(
            "INSERT INTO wiki_people (id, name, phones, emails, handles) \
             VALUES ('person_n', 'Nick', '[\"(512) 555-0142\"]', '[\"Nick@Example.com\"]', '[\"+15125550199\"]')",
        )
        .execute(&pool)
        .await
        .unwrap();
        resolve_people(&Database::from_pool(pool.clone()), window()).await.unwrap();
        assert_eq!(
            people(&pool).await[0].1,
            serde_json::json!(["+15125550142", "+15125550199", "nick@example.com"])
        );
    }

    /// Sharing a meeting is two-way contact, but you and a room are not attendees.
    #[sqlx::test]
    async fn a_meeting_makes_a_person_but_not_you_or_the_room(pool: PgPool) {
        email(&pool, "mine", "sent", "me@example.com", &[], &[], 40).await; // how the box knows your address
        sqlx::query(
            "INSERT INTO data_calendar_event \
               (id, title, attendee_identifiers, started_at, ended_at, source_stream_id, source_table, source_provider) \
             VALUES ('7a1c3e0e-0000-4000-8000-000000000001', 'Lunch', $1, now(), now() + interval '1 hour', 's1', 'gcal', 'google')",
        )
        .bind(serde_json::json!(["me@example.com", "c_1@resource.calendar.google.com", "dana.okafor@example.com"]))
        .execute(&pool)
        .await
        .unwrap();
        resolve_people(&Database::from_pool(pool.clone()), window()).await.unwrap();
        assert_eq!(
            people(&pool).await,
            [("Dana Okafor".to_string(), serde_json::json!(["dana.okafor@example.com"]))]
        );
    }
}
