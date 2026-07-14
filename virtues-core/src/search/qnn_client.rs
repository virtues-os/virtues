//! Native client for the Dragon QNN serving daemon (`crates/virtues-qnnd`).
//!
//! On the Radxa Dragon Q6A the embedder and reranker don't speak HTTP — they
//! run on the Hexagon v68 NPU behind `virtues-qnnd`, a tiny binary-protocol
//! daemon on loopback TCP. This module is the client for it: it tokenizes text,
//! packs int32 token IDs exactly the way the compiled context binaries expect,
//! does the RPC, and (for the reranker) computes ColBERT late-interaction
//! MaxSim over the returned per-token embeddings.
//!
//! It is selected by `VIRTUES_QNND_ADDR` (e.g. `127.0.0.1:7788`); when unset,
//! `embedder.rs`/`reranker.rs` use their HTTP sidecar path instead. Kept
//! dependency-light (tokio + tokenizers only) so it can be validated standalone
//! against the live daemon.
//!
//! ## Wire protocol (little-endian, matches `qnn_server.cpp`)
//! ```text
//! request : u32 model_idx | u32 payload_bytes | payload  (concatenated int32
//!                                                          token-id tensors;
//!                                                          batch = bytes / per-
//!                                                          input size)
//! response: u32 status(0=ok) | u32 payload_bytes | payload (concatenated fp32
//!                                                            outputs)
//! ```
//! Model index 0 = gte-small embed (128 tok in → 384-d vector out). Model index
//! 1 = colbert@256 rerank (256 tok in → 256×96 token-embeddings out).
//!
//! ## Packing (ground truth: the on-device `e2e_demo.py` / `index_store.py`)
//! - **gte** is symmetric (query == doc): `[CLS] + tile/trim-to-126 + [SEP]` =
//!   128 tokens. Short inputs are TILED to fill (not PAD-padded) so mean-pooling
//!   isn't diluted — the model takes only `input_ids`, no attention mask.
//! - **colbert query**: `[CLS, QM] + ids`, trim/MASK-pad to 32, then PAD to 256;
//!   all first 32 positions are "valid" (ColBERT query augmentation counts the
//!   MASK tokens). **colbert doc**: `[CLS, DM] + ids`, trim to 255 `+ [SEP]`,
//!   PAD to 256; valid = real length. Token embeddings come back L2-normalized,
//!   so MaxSim is a plain dot product.

use anyhow::{anyhow, Context, Result};
use std::path::Path;
use tokenizers::Tokenizer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// BERT special-token IDs + the ColBERT query/doc marker tokens, straight from
// the reference packer. These are properties of the compiled context binaries,
// not of the tokenizer, so they're pinned here rather than read from vocab.
const CLS: i32 = 101;
const SEP: i32 = 102;
const MASK: i32 = 103;
const PAD: i32 = 0;
const QM: i32 = 1; // ColBERT query marker
const DM: i32 = 2; // ColBERT document marker

const GTE_SEQ: usize = 128; // embed input length
const COLBERT_SEQ: usize = 256; // rerank input length
const COLBERT_QLEN: usize = 32; // rerank query length (MASK-augmented)

/// gte-small dense embedding width (native; stored untruncated — gte is not
/// Matryoshka-trained).
pub const GTE_DIM: usize = 384;

/// The embed model this daemon serves, stamped onto every vector it produces
/// (`search_embeddings.model`). Named here rather than in `embedder.rs` because
/// this is the module that knows: the NPU context binary is compiled for exactly
/// this model, and it cannot serve another.
pub const GTE_MODEL: &str = "gte-small";
/// ColBERT per-token embedding width.
const COLBERT_TOK_DIM: usize = 96;

const MODEL_EMBED: u32 = 0;
const MODEL_RERANK: u32 = 1;

/// Client for the QNN daemon. Holds the two tokenizers and the daemon address;
/// connections are opened per-RPC (loopback connect is ~µs, negligible vs. the
/// ~4 ms NPU execute, and it sidesteps reconnect bookkeeping).
pub struct QnnClient {
    addr: String,
    gte_tok: Tokenizer,
    colbert_tok: Tokenizer,
}

impl QnnClient {
    /// `addr` = `host:port` of the daemon. `models_dir` holds the tokenizers:
    /// `tok_gte/tokenizer.json` and `tok_colbert/tokenizer.json` (shipped
    /// alongside the `.bin` context binaries).
    pub fn new(addr: impl Into<String>, models_dir: &Path) -> Result<Self> {
        let load = |sub: &str| -> Result<Tokenizer> {
            let p = models_dir.join(sub).join("tokenizer.json");
            Tokenizer::from_file(&p)
                .map_err(|e| anyhow!("loading tokenizer {}: {e}", p.display()))
        };
        Ok(Self {
            addr: addr.into(),
            gte_tok: load("tok_gte")?,
            colbert_tok: load("tok_colbert")?,
        })
    }

    // ── wire ────────────────────────────────────────────────────────────────

    /// One RPC: send `model_idx` + the packed int32 payload, read back the fp32
    /// output. `inputs` is a batch of equal-length int32 tensors, concatenated.
    async fn rpc(&self, model_idx: u32, payload: &[i32]) -> Result<Vec<f32>> {
        let mut sock = TcpStream::connect(&self.addr)
            .await
            .with_context(|| format!("connecting to QNN daemon at {}", self.addr))?;
        sock.set_nodelay(true).ok();

        let nbytes = (payload.len() * 4) as u32;
        let mut hdr = Vec::with_capacity(8 + payload.len() * 4);
        hdr.extend_from_slice(&model_idx.to_le_bytes());
        hdr.extend_from_slice(&nbytes.to_le_bytes());
        for &v in payload {
            hdr.extend_from_slice(&v.to_le_bytes());
        }
        sock.write_all(&hdr).await.context("sending QNN request")?;

        let mut resp_hdr = [0u8; 8];
        sock.read_exact(&mut resp_hdr).await.context("reading QNN response header")?;
        let status = u32::from_le_bytes(resp_hdr[0..4].try_into().unwrap());
        let out_bytes = u32::from_le_bytes(resp_hdr[4..8].try_into().unwrap()) as usize;
        if status != 0 {
            return Err(anyhow!("QNN daemon returned status {status} (model {model_idx})"));
        }
        let mut buf = vec![0u8; out_bytes];
        sock.read_exact(&mut buf).await.context("reading QNN response payload")?;
        Ok(buf
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect())
    }

    // ── gte embed ─────────────────────────────────────────────────────────────

    /// Pack text into the gte 128-token int32 input: `[CLS] + tile/trim-to-126
    /// + [SEP]`. Short token sequences are tiled to fill (matching the reference
    /// packer) so the daemon always receives exactly 128 tokens.
    fn pack_gte(&self, text: &str) -> Result<Vec<i32>> {
        let enc = self
            .gte_tok
            .encode(text, false)
            .map_err(|e| anyhow!("gte tokenize: {e}"))?;
        let ids = enc.get_ids();
        let body = GTE_SEQ - 2; // 126
        let mut out = Vec::with_capacity(GTE_SEQ);
        out.push(CLS);
        if ids.is_empty() {
            // Degenerate (empty content); fill with PAD so the daemon still gets
            // a well-formed 128-length tensor rather than rejecting the batch.
            out.extend(std::iter::repeat(PAD).take(body));
        } else {
            // Tile the ids cyclically to exactly `body` — same result as the
            // reference `(ids * (body/len + 1))[:body]`.
            for i in 0..body {
                out.push(ids[i % ids.len()] as i32);
            }
        }
        out.push(SEP);
        debug_assert_eq!(out.len(), GTE_SEQ);
        Ok(out)
    }

    /// Embed one string → 384-d L2-normalized vector.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut v = self.rpc(MODEL_EMBED, &self.pack_gte(text)?).await?;
        if v.len() != GTE_DIM {
            return Err(anyhow!("gte embed returned {} dims, expected {GTE_DIM}", v.len()));
        }
        l2_normalize(&mut v);
        Ok(v)
    }

    /// Embed a batch of strings in one RPC → one 384-d vector each (order
    /// preserved). Empty input short-circuits.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut payload = Vec::with_capacity(texts.len() * GTE_SEQ);
        for t in texts {
            payload.extend_from_slice(&self.pack_gte(t)?);
        }
        let out = self.rpc(MODEL_EMBED, &payload).await?;
        if out.len() != texts.len() * GTE_DIM {
            return Err(anyhow!(
                "gte batch returned {} floats, expected {}",
                out.len(),
                texts.len() * GTE_DIM
            ));
        }
        Ok(out
            .chunks_exact(GTE_DIM)
            .map(|c| {
                let mut v = c.to_vec();
                l2_normalize(&mut v);
                v
            })
            .collect())
    }

    // ── colbert rerank ─────────────────────────────────────────────────────────

    /// Pack a ColBERT query: `[CLS, QM] + ids`, trimmed/MASK-padded to 32, then
    /// PAD-padded to 256. All 32 leading positions are query-valid (the MASK
    /// augmentation is part of the query representation), so `valid_len` = 32.
    fn pack_colbert_query(&self, query: &str) -> Result<(Vec<i32>, usize)> {
        let enc = self
            .colbert_tok
            .encode(query, false)
            .map_err(|e| anyhow!("colbert tokenize query: {e}"))?;
        let mut ids: Vec<i32> = vec![CLS, QM];
        ids.extend(enc.get_ids().iter().map(|&x| x as i32));
        ids.truncate(COLBERT_QLEN);
        while ids.len() < COLBERT_QLEN {
            ids.push(MASK);
        }
        ids.resize(COLBERT_SEQ, PAD);
        Ok((ids, COLBERT_QLEN))
    }

    /// Pack a ColBERT document: `[CLS, DM] + ids`, trim to 255 `+ [SEP]`, then
    /// PAD-pad to 256. `valid_len` = real (pre-PAD) length.
    fn pack_colbert_doc(&self, text: &str) -> Result<(Vec<i32>, usize)> {
        let enc = self
            .colbert_tok
            .encode(text, false)
            .map_err(|e| anyhow!("colbert tokenize doc: {e}"))?;
        let mut ids: Vec<i32> = vec![CLS, DM];
        ids.extend(enc.get_ids().iter().map(|&x| x as i32));
        ids.truncate(COLBERT_SEQ - 1);
        ids.push(SEP);
        let valid_len = ids.len();
        ids.resize(COLBERT_SEQ, PAD);
        Ok((ids, valid_len))
    }

    /// Run the colbert model over a 256-token packed input → the valid rows of
    /// the 256×96 token-embedding matrix (a `Vec` of `valid_len` × 96 floats,
    /// row-major).
    async fn colbert_tokens(&self, packed: &[i32], valid_len: usize) -> Result<Vec<f32>> {
        let out = self.rpc(MODEL_RERANK, packed).await?;
        let want = COLBERT_SEQ * COLBERT_TOK_DIM;
        if out.len() != want {
            return Err(anyhow!("colbert returned {} floats, expected {want}", out.len()));
        }
        // Keep only the valid leading rows; the rest are PAD positions.
        out.truncate_rows(valid_len, COLBERT_TOK_DIM)
    }

    /// Score `docs` against `query` by ColBERT late interaction (MaxSim): for
    /// each query token, take the max dot-product over the document's tokens,
    /// then sum across query tokens. Returns one score per doc, in input order.
    pub async fn rerank(&self, query: &str, docs: &[String]) -> Result<Vec<f32>> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }
        let (qp, qvalid) = self.pack_colbert_query(query)?;
        let qtok = self.colbert_tokens(&qp, qvalid).await?; // qvalid × 96

        let mut scores = Vec::with_capacity(docs.len());
        for doc in docs {
            let (dp, dvalid) = self.pack_colbert_doc(doc)?;
            let dtok = self.colbert_tokens(&dp, dvalid).await?; // dvalid × 96
            scores.push(maxsim(&qtok, qvalid, &dtok, dvalid, COLBERT_TOK_DIM));
        }
        Ok(scores)
    }
}

/// ColBERT MaxSim over row-major token-embedding matrices. Token embeddings are
/// already L2-normalized by the model, so a dot product is cosine similarity.
fn maxsim(q: &[f32], nq: usize, d: &[f32], nd: usize, dim: usize) -> f32 {
    let mut total = 0.0f32;
    for i in 0..nq {
        let qi = &q[i * dim..(i + 1) * dim];
        let mut best = f32::NEG_INFINITY;
        for j in 0..nd {
            let dj = &d[j * dim..(j + 1) * dim];
            let mut dot = 0.0f32;
            for k in 0..dim {
                dot += qi[k] * dj[k];
            }
            if dot > best {
                best = dot;
            }
        }
        if best.is_finite() {
            total += best;
        }
    }
    total
}

fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Small helper so `colbert_tokens` reads cleanly: keep the first `rows` of a
/// row-major `rows_total × cols` flat vector.
trait TruncateRows {
    fn truncate_rows(self, rows: usize, cols: usize) -> Result<Vec<f32>>;
}
impl TruncateRows for Vec<f32> {
    fn truncate_rows(mut self, rows: usize, cols: usize) -> Result<Vec<f32>> {
        self.truncate(rows * cols);
        Ok(self)
    }
}
