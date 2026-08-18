//! People Resolution
//!
//! Resolves people from multiple sources to canonical wiki_people entities.
//!
//! ## Sources
//!
//! 1. **Calendar Attendees** - Event attendee emails → wiki_people
//! 2. **Email Senders** - From email addresses → wiki_people
//!
//! ## Process
//!
//! 1. Fetch records in time window (calendar events, emails)
//! 2. Extract emails from records
//! 3. Match against wiki_people by email
//! 4. Create new person entities for unknowns
//! 5. Update source records with resolved person IDs

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

    let mut total_resolved = 0;

    // 1. Resolve from calendar attendees
    total_resolved += resolve_calendar_attendees(db, window).await?;

    // 2. Resolve from email senders
    total_resolved += resolve_email_senders(db, window).await?;

    // 3. Resolve from message senders. Deliberately NOT window-bounded — see below.
    total_resolved += resolve_message_senders(db).await?;

    // 4. Resolve the *recipient* of the messages you sent. Same anti-join shape as
    //    (3), same reason it takes no window.
    total_resolved += resolve_message_recipients(db).await?;

    tracing::info!(
        people_resolved = total_resolved,
        "People resolution completed"
    );

    Ok(total_resolved)
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
    normalize_pending_handles(db).await?;

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
                       min(p.canonical_name) AS canonical_name
                FROM wiki_people p
                CROSS JOIN LATERAL jsonb_array_elements_text(p.handles) AS h(handle)
                GROUP BY h.handle
                HAVING count(DISTINCT p.id) = 1
            )
            SELECT m.id            AS "msg_id!",
                   o.person_id     AS "person_id!",
                   o.canonical_name AS "canonical_name!"
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
        let names: Vec<String> = rows.iter().map(|r| r.canonical_name.clone()).collect();
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
                       min(p.canonical_name) AS canonical_name
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
                       min(o.canonical_name) AS canonical_name
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
                   tp.canonical_name AS "canonical_name!"
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
async fn normalize_pending_handles(db: &Database) -> Result<()> {
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
            return Ok(());
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
            return Ok(());
        }

        if (batch as i64) < MESSAGE_BATCH {
            return Ok(());
        }
    }
}

/// Resolve people from calendar attendees in time window
async fn resolve_calendar_attendees(db: &Database, window: TimeWindow) -> Result<usize> {
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
        match resolve_and_link_event_attendees(db, &event).await {
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

/// Resolve people from email senders in time window
///
/// Links from_email to person entity via wiki_refs.
async fn resolve_email_senders(db: &Database, window: TimeWindow) -> Result<usize> {
    // Fetch emails without resolved from_person_id
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

/// Fetch emails without resolved sender entity reference
async fn fetch_unresolved_emails(db: &Database, window: TimeWindow) -> Result<Vec<EmailRecord>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            e.id,
            e.from_email,
            e.from_name
        FROM data_communication_email e
        WHERE e.occurred_at >= $1
          AND e.occurred_at < $2
          AND e.from_email IS NOT NULL
          AND e.from_email != ''
          AND NOT EXISTS (
              SELECT 1 FROM wiki_refs er
              WHERE er.source_table = 'data_communication_email'
                AND er.source_id = e.id
                AND er.role = 'sender'
          )
        ORDER BY e.occurred_at ASC
        LIMIT 1000
        "#,
        window.start,
        window.end
    )
    .fetch_all(db.pool())
    .await?;

    let emails = rows
        .into_iter()
        .filter_map(|row| {
            Some(EmailRecord {
                id: row.id,
                from_email: row.from_email,
                from_name: row.from_name,
            })
        })
        .collect();

    Ok(emails)
}

/// Resolve email sender and link to person entity via wiki_refs
///
/// Returns true if a new person was created or linked.
async fn resolve_and_link_email_sender(db: &Database, email_record: &EmailRecord) -> Result<bool> {
    let email_lower = email_record.from_email.to_lowercase();

    // Resolve or create person
    let person_id = resolve_or_create_person_with_name(
        db,
        &email_lower,
        email_record.from_name.as_deref(),
    )
    .await?;

    // Link via wiki_refs
    let ref_id = ids::generate_id("eref", &[&email_record.id, &person_id, "sender"]);
    sqlx::query!(
        r#"
        INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at)
        SELECT $1, 'person', $2, 'data_communication_email', $3, 'sender', occurred_at
        FROM data_communication_email WHERE id = $3
        ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING
        "#,
        ref_id,
        person_id,
        email_record.id
    )
    .execute(db.pool())
    .await?;

    tracing::debug!(
        email_id = %email_record.id,
        from_email = %email_record.from_email,
        person_id = %person_id,
        "Linked email sender to person via wiki_refs"
    );

    Ok(true)
}

/// Resolve email to person entity (or create if new), with optional display name
///
/// If the person already exists, updates the canonical name if a better name is provided.
async fn resolve_or_create_person_with_name(
    db: &Database,
    email: &str,
    display_name: Option<&str>,
) -> Result<String> {
    // Check if person exists with this email
    let existing = sqlx::query!(
        r#"
        SELECT id, canonical_name
        FROM wiki_people
        WHERE emails @> to_jsonb($1::text)
        LIMIT 1
        "#,
        email
    )
    .fetch_optional(db.pool())
    .await?;

    if let Some(row) = existing {
        let person_id = row.id;

        // Update canonical name if we have a better one (from email header vs extracted from email)
        if let Some(name) = display_name {
            let current_name = row.canonical_name;
            // Only update if current name looks like it was extracted from email (no spaces, or matches email pattern)
            let name_trimmed = name.trim();
            if !name_trimmed.is_empty()
                && !current_name.contains(' ')
                && name_trimmed.contains(' ')
            {
                sqlx::query!(
                    r#"
                    UPDATE wiki_people
                    SET canonical_name = $1,
                        updated_at = now()
                    WHERE id = $2
                    "#,
                    name_trimmed,
                    person_id
                )
                .execute(db.pool())
                .await?;

                tracing::debug!(
                    person_id = %person_id,
                    old_name = %current_name,
                    new_name = %name_trimmed,
                    "Updated person canonical name from email header"
                );
            }
        }

        return Ok(person_id);
    }

    // Create new person entity
    let canonical_name = display_name
        .filter(|n| !n.trim().is_empty())
        .map(|n| n.trim().to_string())
        .unwrap_or_else(|| extract_name_from_email(email));

    let emails_json = serde_json::json!([email]);

    let person_id = ids::generate_id(ids::WIKI_PERSON_PREFIX, &[email]);

    sqlx::query!(
        r#"
        INSERT INTO wiki_people (
            id,
            canonical_name,
            emails
        ) VALUES ($1, $2, $3)
        ON CONFLICT (id) DO NOTHING
        RETURNING id
        "#,
        person_id,
        canonical_name,
        emails_json,
    )
    .fetch_optional(db.pool())
    .await?;

    tracing::info!(
        email = %email,
        person_id = %person_id,
        canonical_name = %canonical_name,
        source = "email_sender",
        "Created new person entity"
    );

    Ok(person_id)
}

/// Calendar event with attendees
#[derive(Debug)]
struct CalendarEvent {
    id: Uuid,
    attendee_identifiers: Vec<String>,
}

/// Fetch calendar events in time window
async fn fetch_calendar_events(db: &Database, window: TimeWindow) -> Result<Vec<CalendarEvent>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            attendee_identifiers
        FROM data_calendar_event
        WHERE started_at >= $1
          AND started_at < $2
          AND jsonb_array_length(attendee_identifiers) > 0
        "#,
        window.start,
        window.end,
    )
    .fetch_all(db.pool())
    .await?;

    let events = rows
        .into_iter()
        .filter_map(|row| {
            let identifiers: Vec<String> = serde_json::from_value(row.attendee_identifiers).ok()?;
            let id = Uuid::parse_str(&row.id).ok()?;
            Some(CalendarEvent {
                id,
                attendee_identifiers: identifiers,
            })
        })
        .collect();

    Ok(events)
}

/// Resolve all attendees for an event and link via wiki_refs
///
/// Returns the number of unique people resolved.
async fn resolve_and_link_event_attendees(db: &Database, event: &CalendarEvent) -> Result<usize> {
    if event.attendee_identifiers.is_empty() {
        return Ok(0);
    }

    let mut unique_people = std::collections::HashSet::new();
    let event_id_str = event.id.to_string();

    // Fetch event start_time for the wiki_refs timestamp
    let timestamp: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT started_at FROM data_calendar_event WHERE id = $1",
    )
    .bind(&event_id_str)
    .fetch_optional(db.pool())
    .await?
    .flatten();

    for email in &event.attendee_identifiers {
        let email_lower = email.to_lowercase();

        match resolve_or_create_person(db, &email_lower).await {
            Ok(person_id) => {
                if unique_people.insert(person_id.clone()) {
                    // Create entity_reference for this attendee
                    let ref_id = ids::generate_id("eref", &[&event_id_str, &person_id, "attendee"]);
                    sqlx::query!(
                        r#"
                        INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at)
                        VALUES ($1, 'person', $2, 'data_calendar_event', $3, 'attendee', $4)
                        ON CONFLICT (entity_id, source_table, source_id, role) DO NOTHING
                        "#,
                        ref_id,
                        person_id,
                        event_id_str,
                        timestamp
                    )
                    .execute(db.pool())
                    .await?;
                }
            }
            Err(e) => {
                tracing::warn!(
                    email = %email,
                    event_id = %event.id,
                    error = %e,
                    "Failed to resolve person for attendee"
                );
            }
        }
    }

    if !unique_people.is_empty() {
        tracing::debug!(
            event_id = %event.id,
            people_count = unique_people.len(),
            "Linked attendees to calendar event via wiki_refs"
        );
    }

    Ok(unique_people.len())
}

/// Resolve email to person entity (or create if new)
///
/// Returns the person entity ID (format: person_{hash16}).
async fn resolve_or_create_person(db: &Database, email: &str) -> Result<String> {
    // Check if person exists with this email
    let existing = sqlx::query!(
        r#"
        SELECT id
        FROM wiki_people
        WHERE emails @> to_jsonb($1::text)
        LIMIT 1
        "#,
        email
    )
    .fetch_optional(db.pool())
    .await?;

    if let Some(row) = existing {
        let id_str = row.id;
        tracing::debug!(
            email = %email,
            person_id = %id_str,
            "Found existing person entity"
        );
        return Ok(id_str);
    }

    // Create new person entity
    let canonical_name = extract_name_from_email(email);

    let emails_json = serde_json::json!([email]);

    // Generate ID with proper prefix (person_{hash16})
    let person_id = ids::generate_id(ids::WIKI_PERSON_PREFIX, &[email]);
    let row = sqlx::query!(
        r#"
        INSERT INTO wiki_people (
            id,
            canonical_name,
            emails
        ) VALUES (
            $1, $2, $3
        )
        RETURNING id
        "#,
        person_id,
        canonical_name,
        emails_json,
    )
    .fetch_one(db.pool())
    .await?;

    let person_id_str = row.id;

    tracing::info!(
        email = %email,
        person_id = %person_id_str,
        canonical_name = %canonical_name,
        "Created new person entity"
    );

    Ok(person_id_str)
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
            "INSERT INTO wiki_people (id, canonical_name, handles)
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
