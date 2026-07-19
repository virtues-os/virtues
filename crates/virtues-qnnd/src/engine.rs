//! NPU inference engine client — tokenization, packing, and scoring around the
//! C++ QNN daemon's binary TCP loop.
//!
//! Moved verbatim from virtues-core's `search/qnn_client.rs` when the box was
//! consolidated onto one HTTP inference contract: the intelligence (tokenizers,
//! packing rules, ColBERT MaxSim) now lives HERE, next to the daemon it drives,
//! and the box speaks plain `/v1/embeddings` + `/v1/rerank` to this process
//! (see `http.rs`) exactly as it does to llama-server.
//!
//! ## Wire protocol (little-endian, matches `csrc/qnn_server.cpp`)
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
//!
//! The packers are pure functions over token ids so they unit-test without
//! tokenizer files (which ship with the models, not the repo).

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

/// The embed model this daemon serves — what `/v1/models` reports, and thus
/// what the box stamps onto every vector (`search_embeddings.model`). The NPU
/// context binary is compiled for exactly this model and can serve no other.
pub const GTE_MODEL: &str = "gte-small";

/// The rerank model, for the `/v1/rerank` response's `model` field.
pub const COLBERT_MODEL: &str = "answerai-colbert-small-v1@256";

/// ColBERT per-token embedding width.
const COLBERT_TOK_DIM: usize = 96;

const MODEL_EMBED: u32 = 0;
const MODEL_RERANK: u32 = 1;

// ── pure packers ─────────────────────────────────────────────────────────────

/// Pack token ids into the gte 128-token int32 input: `[CLS] + tile/trim-to-126
/// + [SEP]`. Short sequences are tiled cyclically to fill (matching the
/// reference packer's `(ids * (body/len + 1))[:body]`).
fn pack_gte_ids(ids: &[u32]) -> Vec<i32> {
    let body = GTE_SEQ - 2; // 126
    let mut out = Vec::with_capacity(GTE_SEQ);
    out.push(CLS);
    if ids.is_empty() {
        // Degenerate (empty content); fill with PAD so the daemon still gets a
        // well-formed 128-length tensor rather than rejecting the batch.
        out.extend(std::iter::repeat(PAD).take(body));
    } else {
        for i in 0..body {
            out.push(ids[i % ids.len()] as i32);
        }
    }
    out.push(SEP);
    debug_assert_eq!(out.len(), GTE_SEQ);
    out
}

/// Pack ColBERT query ids: `[CLS, QM] + ids`, trimmed/MASK-padded to 32, then
/// PAD-padded to 256. All 32 leading positions are query-valid (the MASK
/// augmentation is part of the query representation), so `valid_len` = 32.
fn pack_colbert_query_ids(raw: &[u32]) -> (Vec<i32>, usize) {
    let mut ids: Vec<i32> = vec![CLS, QM];
    ids.extend(raw.iter().map(|&x| x as i32));
    ids.truncate(COLBERT_QLEN);
    while ids.len() < COLBERT_QLEN {
        ids.push(MASK);
    }
    ids.resize(COLBERT_SEQ, PAD);
    (ids, COLBERT_QLEN)
}

/// Pack ColBERT document ids: `[CLS, DM] + ids`, trim to 255 `+ [SEP]`, then
/// PAD-pad to 256. `valid_len` = real (pre-PAD) length.
fn pack_colbert_doc_ids(raw: &[u32]) -> (Vec<i32>, usize) {
    let mut ids: Vec<i32> = vec![CLS, DM];
    ids.extend(raw.iter().map(|&x| x as i32));
    ids.truncate(COLBERT_SEQ - 1);
    ids.push(SEP);
    let valid_len = ids.len();
    ids.resize(COLBERT_SEQ, PAD);
    (ids, valid_len)
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

// ── client ───────────────────────────────────────────────────────────────────

/// Client for the daemon's binary TCP loop. Holds the two tokenizers and the
/// daemon address; connections are opened per-RPC (loopback connect is ~µs,
/// negligible vs. the ~4 ms NPU execute, and it sidesteps reconnect
/// bookkeeping).
pub struct QnnClient {
    addr: String,
    gte_tok: Tokenizer,
    colbert_tok: Tokenizer,
}

impl QnnClient {
    /// `addr` = `host:port` of the daemon loop. `models_dir` holds the
    /// tokenizers: `tok_gte/tokenizer.json` and `tok_colbert/tokenizer.json`
    /// (shipped alongside the `.bin` context binaries).
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

    /// One RPC: send `model_idx` + the packed int32 payload, read back the fp32
    /// output. `payload` is a batch of equal-length int32 tensors, concatenated.
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

    fn gte_ids(&self, text: &str) -> Result<Vec<u32>> {
        Ok(self
            .gte_tok
            .encode(text, false)
            .map_err(|e| anyhow!("gte tokenize: {e}"))?
            .get_ids()
            .to_vec())
    }

    fn colbert_ids(&self, text: &str) -> Result<Vec<u32>> {
        Ok(self
            .colbert_tok
            .encode(text, false)
            .map_err(|e| anyhow!("colbert tokenize: {e}"))?
            .get_ids()
            .to_vec())
    }

    /// Embed one string → 384-d L2-normalized vector.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut v = self.rpc(MODEL_EMBED, &pack_gte_ids(&self.gte_ids(text)?)).await?;
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
            payload.extend_from_slice(&pack_gte_ids(&self.gte_ids(t)?));
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

    /// Run the colbert model over a 256-token packed input → the valid rows of
    /// the 256×96 token-embedding matrix (`valid_len` × 96 floats, row-major).
    async fn colbert_tokens(&self, packed: &[i32], valid_len: usize) -> Result<Vec<f32>> {
        let mut out = self.rpc(MODEL_RERANK, packed).await?;
        let want = COLBERT_SEQ * COLBERT_TOK_DIM;
        if out.len() != want {
            return Err(anyhow!("colbert returned {} floats, expected {want}", out.len()));
        }
        // Keep only the valid leading rows; the rest are PAD positions.
        out.truncate(valid_len * COLBERT_TOK_DIM);
        Ok(out)
    }

    /// Score `docs` against `query` by ColBERT late interaction (MaxSim): for
    /// each query token, take the max dot-product over the document's tokens,
    /// then sum across query tokens. Returns one score per doc, in input order.
    pub async fn rerank(&self, query: &str, docs: &[String]) -> Result<Vec<f32>> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }
        let (qp, qvalid) = pack_colbert_query_ids(&self.colbert_ids(query)?);
        let qtok = self.colbert_tokens(&qp, qvalid).await?; // qvalid × 96

        let mut scores = Vec::with_capacity(docs.len());
        for doc in docs {
            let (dp, dvalid) = pack_colbert_doc_ids(&self.colbert_ids(doc)?);
            let dtok = self.colbert_tokens(&dp, dvalid).await?; // dvalid × 96
            scores.push(maxsim(&qtok, qvalid, &dtok, dvalid, COLBERT_TOK_DIM));
        }
        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gte_pack_tiles_short_input() {
        let packed = pack_gte_ids(&[10, 11, 12]);
        assert_eq!(packed.len(), GTE_SEQ);
        assert_eq!(packed[0], CLS);
        assert_eq!(packed[GTE_SEQ - 1], SEP);
        // Body tiles cyclically: 10,11,12,10,11,12,…
        assert_eq!(&packed[1..7], &[10, 11, 12, 10, 11, 12]);
        // 126 body positions: position 1+125 = ids[125 % 3] = ids[2] = 12
        assert_eq!(packed[126], 12);
    }

    #[test]
    fn gte_pack_empty_pads() {
        let packed = pack_gte_ids(&[]);
        assert_eq!(packed.len(), GTE_SEQ);
        assert_eq!(packed[0], CLS);
        assert!(packed[1..GTE_SEQ - 1].iter().all(|&t| t == PAD));
        assert_eq!(packed[GTE_SEQ - 1], SEP);
    }

    #[test]
    fn gte_pack_trims_long_input() {
        let long: Vec<u32> = (1000..2000).collect();
        let packed = pack_gte_ids(&long);
        assert_eq!(packed.len(), GTE_SEQ);
        assert_eq!(packed[1], 1000);
        assert_eq!(packed[126], 1125); // 126th body token = ids[125]
    }

    #[test]
    fn colbert_query_pack_mask_augments() {
        let (packed, valid) = pack_colbert_query_ids(&[50, 51]);
        assert_eq!(packed.len(), COLBERT_SEQ);
        assert_eq!(valid, COLBERT_QLEN);
        assert_eq!(&packed[..4], &[CLS, QM, 50, 51]);
        // MASK-padded to 32, PAD beyond.
        assert!(packed[4..COLBERT_QLEN].iter().all(|&t| t == MASK));
        assert!(packed[COLBERT_QLEN..].iter().all(|&t| t == PAD));
    }

    #[test]
    fn colbert_doc_pack_terminates_with_sep() {
        let (packed, valid) = pack_colbert_doc_ids(&[50, 51, 52]);
        assert_eq!(packed.len(), COLBERT_SEQ);
        assert_eq!(valid, 6); // CLS, DM, 50, 51, 52, SEP
        assert_eq!(&packed[..6], &[CLS, DM, 50, 51, 52, SEP]);
        assert!(packed[6..].iter().all(|&t| t == PAD));

        // Long doc: trimmed to 255 + SEP, valid = 256.
        let long: Vec<u32> = (0..400).collect();
        let (packed, valid) = pack_colbert_doc_ids(&long);
        assert_eq!(valid, COLBERT_SEQ);
        assert_eq!(packed[COLBERT_SEQ - 1], SEP);
    }

    #[test]
    fn maxsim_sums_per_query_maxima() {
        // 2 query tokens, 2 doc tokens, dim 2. Unit-ish vectors.
        let q = [1.0, 0.0, 0.0, 1.0]; // q0=(1,0) q1=(0,1)
        let d = [0.6, 0.8, 1.0, 0.0]; // d0=(.6,.8) d1=(1,0)
        // q0: max(0.6, 1.0)=1.0 ; q1: max(0.8, 0.0)=0.8 → 1.8
        let s = maxsim(&q, 2, &d, 2, 2);
        assert!((s - 1.8).abs() < 1e-6);
    }

    #[test]
    fn l2_normalize_unit_norm() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
    }
}
