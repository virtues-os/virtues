//! The magnet: a container that fills itself.
//!
//! A notebook is a folder — you drag material in. Turn `auto_add_materials` on
//! and it becomes a magnet: material that resembles what is already inside
//! attaches on its own. A story uses the identical machinery to gather the
//! evidence for its claim. One primitive, two surfaces, and this is the whole
//! of it:
//!
//! ```text
//!   centroid  =  mean( embed(seed) , embeddings of the members )
//!   recall    =  hybrid_search(seed)  ∪  ANN(centroid)
//!   admit     =  cross-encoder(claim, candidate) >= THRESHOLD
//! ```
//!
//! ## Why not just cosine against the centroid
//!
//! Because it does not work, and the first run proved it. A story titled
//! "Buying a house in Bouldin Creek" scored its one true piece of evidence —
//! *"That Bouldin Creek house on S 3rd just came back on the market"* — at
//! 0.513, while an unrelated design meeting sat at 0.450. The right answer
//! ranked first and the gap was five points. There is no absolute threshold you
//! can hang on a spread that thin.
//!
//! "Bouldin Creek" is a LEXICAL gift, and a pure-vector magnet throws it away.
//! So recall comes from the hybrid stack that already exists — BM25 + dense +
//! RRF — with the centroid ANN unioned in, because the centroid finds evidence
//! that shares no words with the thesis but belongs to it anyway. Lexical for
//! the obvious, vector for the oblique.
//!
//! ## Why the magnet reranks for itself
//!
//! `SearchResult.score` is min-max normalised *within its result set* — the top
//! hit is ≈1.0 whether or not it is any good, and `search()` only reranks when
//! the top-1/top-2 margin is tight. Both are right for a search box and fatal
//! here: gate on that score and the magnet admits the best of a bad batch,
//! every time, forever.
//!
//! Cross-encoder logits are absolute and comparable across runs, which is what
//! an admission decision actually requires. The magnet runs on a cron over a
//! bounded pool — exactly the budget where a cross-encoder is affordable.
//!
//! ## Why there is a seed
//!
//! An empty notebook has no members to average, so a pure member-mean magnet
//! can never start — it needs material to attract material. The seed breaks
//! that circle: a notebook's name and instructions, or a story's title and
//! thesis, is *already* a statement of what belongs. "My prayer life" is a
//! usable query on day one. The members then pull the centroid toward what the
//! container has actually turned out to be about, which is the part the user
//! could not have written down.
//!
//! ## The self-attachment trap
//!
//! A notebook's own chat messages are embedded, and they are *by construction*
//! the nearest neighbours of its own centroid — you have been talking about
//! exactly this subject in exactly this notebook. Unguarded, the magnet
//! attaches the notebook's conversation back into the notebook, forever, and
//! the centroid drifts toward its own echo. `EXCLUDE_OWN_CHAT` is not a nicety.
//!
//! ## What it attaches
//!
//! Routes, of the form `/record/{ontology}/{record_id}` — the same route the
//! model cites with. A member and a citation are the same primitive: the story
//! body cites its evidence by emitting a member's route. Nothing is attached
//! that cannot be opened.
//!
//! ## Reversibility
//!
//! Every auto-attached row carries `added_by = 'magnet'` and the rerank
//! confidence that admitted it. The UI shows it as machine-added and removes it
//! in one click, and the threshold can be re-tuned against what it actually let
//! in rather than argued about in the abstract.

use crate::error::Result;
use crate::search::embedder::get_embedder;
use pgvector::HalfVector;
use sqlx::{PgPool, Row};

/// The cross-encoder logit a candidate must clear to be attached.
///
/// A logit, not a probability, and that is the point: it is an ABSOLUTE
/// statement by the model about this (query, document) pair, comparable across
/// runs and containers. A probability-looking 0.6 sounds safe and is not — it
/// is logit 0.4, which is barely above indifference.
///
/// The number comes from measured separation, not taste. Against the claim
/// "Buying a house in Bouldin Creek", over this box's real corpus:
///
/// ```text
///   +2.44   "That Bouldin Creek house on S 3rd just came back on the market"   TRUE
///   +0.99   "Perfect — 1847 S 3rd St. I'll be out front"                       TRUE
///   +0.91   "theme"                                                            junk
///   +0.62   "yeah the step 3 completion rate is brutal"                        junk
///   -0.21   "back in black lyrics"                                             junk
///   -2.17   "Design Team Standup — daily sync, blockers, progress"             junk
/// ```
///
/// Read that honestly: the worst true positive beats the best junk by **0.08**.
/// A one-word chat message ("theme") is nearly indistinguishable from a street
/// address, because stripped of their context both are fragments, and a
/// cross-encoder cannot judge a fragment. **Raw chat messages are not evidence.**
///
/// So 2.0 is a PRECISION choice with its eyes open: it admits the unambiguous
/// match and misses the address. A magnet that admits junk is worse than a
/// folder — the user must then *unpick* it, and nobody unpicks. Recall comes
/// back not by lowering this number but by giving the reranker documents worth
/// judging: `wiki_events`, which carry real summaries ("Phone call with Rachel
/// Torres") instead of five stray characters.
///
/// Every attachment stores the score that admitted it, so this gets fixed by
/// looking at what it did, not by arguing about it.
const THRESHOLD: f32 = 2.0;

/// Candidates drawn per arm before reranking. Not a quality bar — a blast
/// radius, and the reranker's bill.
const RECALL: i64 = 60;

/// Most attachments per run. A mis-tuned threshold should leave a mess you can
/// see and delete, not one that takes an afternoon.
const MAX_ATTACH: usize = 50;

/// Which container the magnet is running for. Notebooks and stories are
/// different tables with different columns, and the same magnet.
#[derive(Debug, Clone, Copy)]
pub struct Target {
    pub table: &'static str,
    pub members_table: &'static str,
    pub owner_col: &'static str,
    /// SQL yielding the container's own statement of what belongs in it — the
    /// cold-start seed, and a permanent anchor against member drift. Used for
    /// RECALL, where more words help BM25 and the dense arm.
    pub seed_sql: &'static str,
    /// The claim, crisply. Used for ADMISSION, where more words hurt: measured
    /// against this corpus, adding a story's prose thesis to the rerank query
    /// dropped the separation between true evidence and junk from +0.08 to
    /// -0.53 — junk outranked a true positive. A cross-encoder wants a sharp
    /// question, not an essay.
    pub claim_sql: &'static str,
    /// A notebook holds its chats; a story does not. See the self-attachment
    /// trap above.
    pub exclude_own_chat: bool,
}

pub const NOTEBOOK: Target = Target {
    table: "app_notebooks",
    members_table: "app_notebook_items",
    owner_col: "notebook_id",
    seed_sql: "name || COALESCE(E'\n' || instructions, '')",
    claim_sql: "name",
    exclude_own_chat: true,
};

pub const STORY: Target = Target {
    table: "wiki_stories",
    members_table: "wiki_story_members",
    owner_col: "story_id",
    seed_sql: "title || COALESCE(E'\n' || thesis, '')",
    claim_sql: "title",
    exclude_own_chat: false,
};

/// Recompute the centroid: the seed, plus every member that resolves to an
/// embedding, averaged and normalised.
///
/// Members that carry no embedding (a pinned person, a place) are silently
/// skipped rather than faked. A place has no prose; there is nothing to average
/// in. It still belongs to the notebook — it simply does not steer the magnet.
pub async fn recompute_centroid(pool: &PgPool, t: Target, owner_id: &str) -> Result<bool> {
    let seed: Option<String> = sqlx::query_scalar(&format!(
        "SELECT {} FROM {} WHERE id = $1",
        t.seed_sql, t.table
    ))
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;

    let Some(seed) = seed else { return Ok(false) };

    // Member embeddings. Only `/record/{ontology}/{id}` routes resolve — that
    // is what the magnet itself attaches, so a container converges on a
    // centroid built from the evidence it has actually accumulated.
    let member_vecs: Vec<Vec<f32>> = sqlx::query(&format!(
        r#"
        SELECT v.embedding
        FROM {members} m
        JOIN search_embeddings e
          ON  '/record/' || e.ontology || '/' || e.record_id = m.url
        JOIN search_vectors v ON v.embedding_id = e.id
        WHERE m.{owner} = $1
        "#,
        members = t.members_table,
        owner = t.owner_col
    ))
    .bind(owner_id)
    .fetch_all(pool)
    .await?
    .iter()
    // Widened to f32 for the averaging: the corpus is stored fp16, but summing
    // N of them and rounding at every step accumulates error. Round once, at
    // the end, on the way back into the column.
    .map(|r| {
        r.get::<HalfVector, _>("embedding")
            .to_vec()
            .iter()
            .map(|h| h.to_f32())
            .collect()
    })
    .collect();

    let embedder = get_embedder().await?;
    let seed_vec = embedder.embed_batch_async(vec![seed]).await?;
    let Some(seed_vec) = seed_vec.into_iter().next() else {
        return Ok(false);
    };

    let dim = seed_vec.len();
    let mut sum = seed_vec;
    for v in &member_vecs {
        if v.len() != dim {
            // A vector from another model — refuse rather than average across
            // two spaces, which produces a number that means nothing.
            tracing::warn!(
                expected = dim,
                found = v.len(),
                "magnet: dimension mismatch, skipping member vector"
            );
            continue;
        }
        for (s, x) in sum.iter_mut().zip(v) {
            *s += x;
        }
    }

    let n = 1 + member_vecs.len();
    for s in sum.iter_mut() {
        *s /= n as f32;
    }
    normalize(&mut sum);

    sqlx::query(&format!(
        "UPDATE {} SET centroid = $1, dirty_at = NULL, updated_at = now() WHERE id = $2",
        t.table
    ))
    .bind(HalfVector::from_f32_slice(&sum))
    .bind(owner_id)
    .execute(pool)
    .await?;

    Ok(true)
}

/// A candidate record, before the reranker has had its say.
struct Candidate {
    url: String,
    text: String,
    occurred_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Attach material that belongs. Returns how many were attached.
pub async fn attach(pool: &PgPool, t: Target, owner_id: &str) -> Result<u32> {
    let row = sqlx::query(&format!(
        "SELECT {} AS seed, {} AS claim, centroid FROM {} WHERE id = $1 AND auto_add_materials",
        t.seed_sql, t.claim_sql, t.table
    ))
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;

    // No such container, or the magnet is off. Not an error — nothing to do.
    let Some(row) = row else { return Ok(0) };
    let seed: String = row.get("seed");
    let claim: String = row.get("claim");
    let centroid: Option<HalfVector> = row.get("centroid");

    // ---- Recall: two arms, unioned. ----------------------------------------
    //
    // The hybrid arm (BM25 + dense + RRF) catches evidence that SAYS the words
    // — "Bouldin Creek" in a text message. The centroid arm catches evidence
    // that shares no words with the thesis but sits among what the container has
    // already gathered. Neither finds the other's hits, which is the argument
    // for running both.
    let own_chat_filter = if t.exclude_own_chat {
        // A notebook's own chat messages are, by construction, the nearest
        // neighbours of its own centroid — you have been talking about exactly
        // this subject in exactly this notebook. Unguarded, the magnet attaches
        // the conversation back into the notebook and the centroid drifts into
        // its own echo. This is not a nicety; on the first live run it was the
        // single highest-scoring candidate in the entire corpus.
        "AND NOT (e.ontology = 'app_chat_message' AND EXISTS (
             SELECT 1 FROM app_chat_messages cm
             JOIN app_chats c ON c.id = cm.chat_id
             WHERE cm.id = e.record_id AND c.notebook_id = $1))"
    } else {
        ""
    };

    let mut candidates: std::collections::HashMap<String, Candidate> = Default::default();

    if let Some(centroid) = &centroid {
        // DISTINCT ON, not GROUP BY: a record is many chunks, and the reranker
        // must read the chunk that actually MATCHED. Aggregating the text with
        // MIN() picks the alphabetically-first chunk instead — which is how a
        // chat about iroh networking came back scored against a house hunt at
        // 0.75 when the chunk that mattered scored 0.23.
        let sql = format!(
            r#"
            WITH best_chunk AS (
                SELECT DISTINCT ON (e.ontology, e.record_id)
                       '/record/' || e.ontology || '/' || e.record_id AS url,
                       e.timestamp AS occurred_at,
                       COALESCE(e.content, e.preview, e.title, '') AS text,
                       v.embedding <=> $2 AS dist
                FROM search_vectors v
                JOIN search_embeddings e ON e.id = v.embedding_id
                WHERE NOT EXISTS (
                          SELECT 1 FROM {members} m
                          WHERE m.{owner} = $1
                            AND m.url = '/record/' || e.ontology || '/' || e.record_id)
                      {own_chat}
                ORDER BY e.ontology, e.record_id, v.embedding <=> $2
            )
            SELECT url, occurred_at, text FROM best_chunk
            ORDER BY dist
            LIMIT $3
            "#,
            members = t.members_table,
            owner = t.owner_col,
            own_chat = own_chat_filter,
        );

        for r in sqlx::query(&sql)
            .bind(owner_id)
            .bind(centroid)
            .bind(RECALL)
            .fetch_all(pool)
            .await?
        {
            let url: String = r.get("url");
            candidates.insert(
                url.clone(),
                Candidate {
                    url,
                    text: r.get("text"),
                    occurred_at: r.try_get("occurred_at").ok().flatten(),
                },
            );
        }
    }

    // The lexical/dense hybrid, over the seed text. Reused wholesale — the
    // magnet has no business reimplementing retrieval, and a weaker private
    // copy of it is exactly what the first version was.
    let engine = crate::search::query::SemanticSearchEngine::new(std::sync::Arc::new(pool.clone()));
    let hits = engine
        .search(&seed, None, None, None, None, None, Some(RECALL))
        .await?;

    for h in hits {
        let url = format!("/record/{}/{}", h.ontology, h.record_id);
        if candidates.contains_key(&url) {
            continue;
        }
        let Some(text) = h.content.clone().or_else(|| h.preview.clone()) else {
            continue;
        };
        candidates.insert(
            url.clone(),
            Candidate {
                url,
                text,
                occurred_at: h
                    .timestamp
                    .as_deref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.with_timezone(&chrono::Utc)),
            },
        );
    }

    if candidates.is_empty() {
        return Ok(0);
    }

    // The hybrid arm does not know about members or the own-chat rule, so its
    // hits are filtered here rather than in its SQL.
    let existing: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT url FROM {} WHERE {} = $1",
        t.members_table, t.owner_col
    ))
    .bind(owner_id)
    .fetch_all(pool)
    .await?;
    for url in existing {
        candidates.remove(&url);
    }

    if t.exclude_own_chat {
        let own: Vec<String> = sqlx::query_scalar(
            "SELECT '/record/app_chat_message/' || cm.id
             FROM app_chat_messages cm
             JOIN app_chats c ON c.id = cm.chat_id
             WHERE c.notebook_id = $1",
        )
        .bind(owner_id)
        .fetch_all(pool)
        .await?;
        for url in own {
            candidates.remove(&url);
        }
    }

    let pool_vec: Vec<Candidate> = candidates.into_values().collect();
    if pool_vec.is_empty() {
        return Ok(0);
    }

    // ---- Admission: the cross-encoder, on an absolute scale. ----------------
    let reranker = crate::search::reranker::get_reranker().await?;
    let docs: Vec<String> = pool_vec.iter().map(|c| c.text.clone()).collect();
    let scores = reranker.rerank_async(&claim, &docs).await?;

    // Gate on the raw logit (absolute, comparable across runs); store the
    // sigmoid, which is the 0–1 confidence the UI can show a human.
    let mut admitted: Vec<(&Candidate, f64)> = scores
        .iter()
        .filter(|s| s.score >= THRESHOLD)
        .map(|s| (&pool_vec[s.index], sigmoid(s.score)))
        .collect();
    admitted.sort_by(|a, b| b.1.total_cmp(&a.1));
    admitted.truncate(MAX_ATTACH);

    let mut attached = 0u32;
    for (c, score) in admitted {
        // A story records WHEN its evidence happened — it is a shape in time.
        // A notebook is spatial and has no such axis, so it takes sort_order.
        let insert = if t.members_table == "wiki_story_members" {
            sqlx::query(
                "INSERT INTO wiki_story_members (story_id, url, added_by, similarity, occurred_at)
                 VALUES ($1, $2, 'magnet', $3, $4)
                 ON CONFLICT (story_id, url) DO NOTHING",
            )
            .bind(owner_id)
            .bind(&c.url)
            .bind(score)
            .bind(c.occurred_at)
        } else {
            sqlx::query(
                "INSERT INTO app_notebook_items (notebook_id, url, role, added_by, similarity, sort_order)
                 VALUES ($1, $2, 'library', 'magnet', $3,
                         (SELECT COALESCE(MAX(sort_order), -1) + 1
                            FROM app_notebook_items WHERE notebook_id = $1))
                 ON CONFLICT (notebook_id, url) DO NOTHING",
            )
            .bind(owner_id)
            .bind(&c.url)
            .bind(score)
        };

        attached += insert.execute(pool).await?.rows_affected() as u32;
    }

    if attached > 0 {
        // New members move the centroid. Mark it, do not recompute here: a
        // recompute would change what the next attach admits, mid-run.
        sqlx::query(&format!(
            "UPDATE {} SET dirty_at = now(), updated_at = now() WHERE id = $1",
            t.table
        ))
        .bind(owner_id)
        .execute(pool)
        .await?;
    }

    Ok(attached)
}

/// Run the magnet over every container that has it switched on.
pub async fn run_all(pool: &PgPool, t: Target) -> Result<u32> {
    let ids: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT id FROM {} WHERE auto_add_materials",
        t.table
    ))
    .fetch_all(pool)
    .await?;

    let mut total = 0;
    for id in ids {
        // Centroid first: a stale centroid attracts the wrong things, and a
        // container whose members just changed has a stale centroid by
        // definition.
        recompute_centroid(pool, t, &id).await?;
        total += attach(pool, t, &id).await?;
    }
    Ok(total)
}

/// Cross-encoder logits are unbounded; the sigmoid puts them on the [0,1] scale
/// the threshold is expressed in, and preserves their order.
fn sigmoid(logit: f32) -> f64 {
    1.0 / (1.0 + (-(logit as f64)).exp())
}

/// Cosine similarity assumes unit vectors; the centroid of several is not one.
fn normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}
