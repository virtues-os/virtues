//! The magnet: a container that fills itself.
//!
//! A notebook is a folder — you drag material in. Turn `auto_add_materials` on
//! and it becomes a magnet: material that resembles what is already inside
//! attaches on its own. This is the whole of it:
//!
//! ```text
//!   centroid  =  mean( embed(seed) , embeddings of the members )
//!   recall    =  hybrid_search(seed)  ∪  ANN(centroid)
//!   admit     =  rerank(claim, candidate) − anchor_baseline >= DELTA
//! ```
//!
//! ## Why not just cosine against the centroid
//!
//! Because it does not work, and the first run proved it. A notebook seeded
//! "Buying a house in Bouldin Creek" scored its one true piece of evidence —
//! *"That Bouldin Creek house on S 3rd just came back on the market"* — at
//! 0.513, while an unrelated design meeting sat at 0.450. The right answer
//! ranked first and the gap was five points. There is no absolute threshold you
//! can hang on a spread that thin.
//!
//! "Bouldin Creek" is a LEXICAL gift, and a pure-vector magnet throws it away.
//! So recall comes from the hybrid stack that already exists — BM25 + dense +
//! RRF — with the centroid ANN unioned in, because the centroid finds evidence
//! that shares no words with the seed but belongs to it anyway. Lexical for
//! the obvious, vector for the oblique.
//!
//! ## Why the magnet reranks for itself, and against anchors
//!
//! `SearchResult.score` is min-max normalised *within its result set* — the top
//! hit is ≈1.0 whether or not it is any good, and `search()` only reranks when
//! the top-1/top-2 margin is tight. Both are right for a search box and fatal
//! here: gate on that score and the magnet admits the best of a bad batch,
//! every time, forever. So the magnet reranks its own pool.
//!
//! But the reranker's score is NOT an absolute scale. On the box the endpoint
//! is ColBERT MaxSim (`answerai-colbert-small-v1`), whose scores are a large
//! per-query baseline (~28.6) plus a small relevance delta — a perfect match
//! beats an unrelated sentence by tenths of a point out of ~29. The ordering is
//! right; the absolute number is not a threshold you can hang anything on. So
//! rather than assume a scale, the magnet MEASURES the baseline: a few fixed,
//! deliberately-irrelevant `ANCHORS` ride along in every rerank batch, their top
//! score is this query's "irrelevant" floor, and a candidate is admitted only if
//! it clears that floor by `DELTA`. Per-query, and safe on a bad batch — when
//! recall found only junk, the junk scores at the floor and nothing is admitted.
//!
//! ## Why there is a seed
//!
//! An empty notebook has no members to average, so a pure member-mean magnet
//! can never start — it needs material to attract material. The seed breaks
//! that circle: a notebook's name and instructions are *already* a statement of
//! what belongs. "My prayer life" is a usable query on day one, before it holds
//! a single member. The members then pull the centroid toward what the
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
//! model cites with. A member and a citation are the same primitive: chat
//! scoped to a notebook cites its evidence by emitting a member's route. Nothing
//! is attached that cannot be opened.
//!
//! ## Reversibility
//!
//! Every auto-attached row carries `added_by = 'magnet'` and the rerank
//! confidence that admitted it. The UI shows it as machine-added and removes it
//! in one click, and the threshold can be re-tuned against what it actually let
//! in rather than argued about in the abstract.

use crate::error::Result;
use crate::search::embedder::get_embedder;
use crate::search::query::{SearchFilters, SearchResult, SemanticSearchEngine};
use pgvector::{HalfVector, Vector};
use sqlx::{PgPool, Row};

/// How far a candidate must out-score the anchor floor to be admitted. The
/// floor is measured per query (see `ANCHORS`), so this is a margin, not an
/// absolute cutoff. Graded tests against the live ColBERT reranker put real
/// evidence 0.25–0.5 above the anchors and irrelevant text within ~0.05, so a
/// margin here separates them; tune against what a run actually admits.
const DELTA: f32 = 0.15;

/// Sentences with no relation to any container, reranked alongside the real
/// candidates to measure this query's "irrelevant" score. The reranker returns
/// a large baseline plus a small relevance delta; the anchors reveal the
/// baseline so `DELTA` can be applied to the part that carries the signal.
const ANCHORS: &[&str] = &[
    "The quarterly tax filing deadline is next Friday.",
    "A car engine needs a new gasket and flywheel.",
    "Tomorrow's weather forecast calls for scattered rain.",
];

/// Candidates drawn per arm before reranking. Not a quality bar — a blast
/// radius, and the reranker's bill.
const RECALL: i64 = 60;

/// Most attachments per run. A mis-tuned threshold should leave a mess you can
/// see and delete, not one that takes an afternoon.
const MAX_ATTACH: usize = 50;

/// Which container the magnet is running for. Today that is only notebooks
/// (`NOTEBOOK`); the indirection stays so a second container could reuse the
/// same machinery against a different table without touching the magnet.
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
    /// against this corpus, padding the rerank query with prose dropped the
    /// separation between true evidence and junk from +0.08 to -0.53 — junk
    /// outranked a true positive. The reranker wants a sharp question, not an
    /// essay, so this stays a name, not a paragraph.
    pub claim_sql: &'static str,
    /// Whether to exclude the container's own chat messages from recall — they
    /// are by construction the nearest neighbours of its own centroid. See the
    /// self-attachment trap above.
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
}

/// Fold a recall hit into the candidate pool, keyed by its record route so the
/// two arms dedupe against each other. First writer wins (the centroid arm runs
/// first); a hit with no usable text is dropped rather than reranked blind.
fn insert_candidate(map: &mut std::collections::HashMap<String, Candidate>, h: SearchResult) {
    let url = format!("/record/{}/{}", h.ontology, h.record_id);
    if map.contains_key(&url) {
        return;
    }
    // content is the matched chunk; preview/title are the record-level fallbacks
    // for rows that carry no embedded content (matches the old COALESCE).
    let Some(text) = h.content.or(h.preview).or(h.title) else {
        return;
    };
    map.insert(url.clone(), Candidate { url, text });
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

    // ---- Recall: two arms through the shared engine. -----------------------
    //
    // The hybrid arm (BM25 ⊕ dense) catches evidence that SAYS the words —
    // "Bouldin Creek" in a text message. The centroid arm (a pure-vector query)
    // catches evidence that shares no words with the seed but sits among what
    // the container has already gathered. Both now run through the ONE engine's
    // `recall_and_fuse` — the magnet keeps no private copy of pgvector retrieval
    // (a second, unmaintained copy is exactly how its centroid dimension silently
    // rotted, and the `search()`-clamped-to-20 starvation lived here too).
    let engine = SemanticSearchEngine::new(std::sync::Arc::new(pool.clone()));

    // Exclude the container's existing members and (for a notebook) its own chat
    // BEFORE recall. These are the nearest neighbours of the container's own
    // centroid, so excluding them post-recall would let them consume the RECALL
    // budget and starve fresh candidates — worse the more the container holds.
    let mut exclude_urls: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT url FROM {} WHERE {} = $1",
        t.members_table, t.owner_col
    ))
    .bind(owner_id)
    .fetch_all(pool)
    .await?;
    if t.exclude_own_chat {
        let own: Vec<String> = sqlx::query_scalar(
            "SELECT '/record/app_chat/' || c.id FROM app_chats c WHERE c.notebook_id = $1",
        )
        .bind(owner_id)
        .fetch_all(pool)
        .await?;
        exclude_urls.extend(own);
    }
    let filters = SearchFilters { exclude_urls, ..Default::default() };
    let mut candidates: std::collections::HashMap<String, Candidate> = Default::default();

    // Centroid arm first (it wins ties). Empty terms → dense-only fusion, i.e. a
    // nearest-neighbour search against the container's centre of gravity.
    if let Some(centroid) = &centroid {
        let cvec = Vector::from(
            centroid.to_vec().iter().map(|h| h.to_f32()).collect::<Vec<f32>>(),
        );
        for h in engine.recall_and_fuse(&cvec, &[], &filters, RECALL).await? {
            insert_candidate(&mut candidates, h);
        }
    }

    // Seed arm: the container's own statement of what belongs, embedded as a
    // query and hybrid-retrieved. RECALL is honored here — `search()` would have
    // clamped it to 20 (the old starvation), `recall_and_fuse` does not.
    let embedder = get_embedder().await?;
    let seed_vec = Vector::from(embedder.embed_query_async(&seed).await?);
    let seed_terms = crate::search::bm25::tokens(&seed);
    for h in engine
        .recall_and_fuse(&seed_vec, &seed_terms, &filters, RECALL)
        .await?
    {
        insert_candidate(&mut candidates, h);
    }

    let pool_vec: Vec<Candidate> = candidates.into_values().collect();
    if pool_vec.is_empty() {
        return Ok(0);
    }

    // ---- Admission: the reranker, gated relative to a measured baseline. -----
    //
    // The reranker orders correctly, but its scores are NOT an absolute scale.
    // On the box the endpoint is ColBERT MaxSim (`answerai-colbert-small-v1`),
    // whose scores are a large per-query baseline (~28.6) plus a small relevance
    // delta (a perfect match beats a car-repair sentence by ~0.14 out of ~29).
    // The old gate — `raw >= 2.0`, a cross-encoder logit threshold — let every
    // candidate through, and `sigmoid(28)` stored 1.000 for all of them.
    //
    // So we measure the baseline instead of assuming it. A handful of ANCHORS —
    // sentences deliberately unrelated to anything — ride along in the same
    // rerank batch. Their top score is this query's "irrelevant" floor, and a
    // real candidate is admitted only if it clears that floor by DELTA. This is
    // per-query (the baseline drifts) and it defends against a bad batch: when
    // recall found only junk, the junk scores near the anchors and nothing is
    // admitted — which min-max normalisation alone would not do.
    let reranker = crate::search::reranker::get_reranker().await?;
    let n_real = pool_vec.len();
    let mut docs: Vec<String> = pool_vec.iter().map(|c| c.text.clone()).collect();
    docs.extend(ANCHORS.iter().map(|s| s.to_string()));
    let scores = reranker.rerank_async(&claim, &docs).await?;

    // rerank returns one score per input, indexed into `docs`. Split them back
    // into real candidates and anchors by that index.
    let mut real: Vec<(usize, f32)> = Vec::with_capacity(n_real);
    let mut baseline = f32::MIN;
    for s in &scores {
        if s.index < n_real {
            real.push((s.index, s.score));
        } else {
            baseline = baseline.max(s.score);
        }
    }

    // The floor is only meaningful if the anchors actually came back. If none
    // did (a reranker that dropped inputs), `baseline` is still f32::MIN and the
    // margin gate would admit everything — fail CLOSED instead.
    if baseline == f32::MIN {
        tracing::warn!("magnet: reranker returned no anchor scores; skipping admission");
        return Ok(0);
    }

    // Admit candidates that clear the anchor floor by DELTA. Store the margin
    // above baseline as `similarity` — a small, honest, comparable number, not
    // a saturated sigmoid. Higher margin = better fit; the UI can rank on it.
    let mut admitted: Vec<(&Candidate, f64)> = real
        .into_iter()
        .filter(|(_, score)| score - baseline >= DELTA)
        .map(|(i, score)| (&pool_vec[i], (score - baseline) as f64))
        .collect();
    admitted.sort_by(|a, b| b.1.total_cmp(&a.1));
    admitted.truncate(MAX_ATTACH);

    let mut attached = 0u32;
    for (c, margin) in admitted {
        let insert = sqlx::query(
            "INSERT INTO app_notebook_items (notebook_id, url, role, added_by, sort_order)
             VALUES ($1, $2, 'library', 'magnet',
                     (SELECT COALESCE(MAX(sort_order), -1) + 1
                        FROM app_notebook_items WHERE notebook_id = $1))
             ON CONFLICT (notebook_id, url) DO NOTHING",
        )
        .bind(owner_id)
        .bind(&c.url);
        let _ = margin; // computed for the admission decision; no longer stored

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

/// Cosine similarity assumes unit vectors; the centroid of several is not one.
fn normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}
