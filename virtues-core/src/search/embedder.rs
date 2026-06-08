//! Embedding trait and local implementation via ORT + tokenizers.
//!
//! Model: `google/embeddinggemma-300m` (int8 quantized ONNX export from the
//! `onnx-community/embeddinggemma-300m-ONNX` repo). 768-dim Matryoshka head.
//!
//! ## Output strategy
//!
//! The ONNX export may expose either `sentence_embedding` (B × D, already
//! mean-pooled and L2-normalized) or only `last_hidden_state` (B × T × D).
//! We try the pooled output first, then fall back to manual mean-pool +
//! normalize on the hidden state. That way the same code handles both
//! sentence-transformers-style and HF-transformers-style exports.
//!
//! ## Inputs
//!
//! Only `input_ids` and `attention_mask` are fed. `token_type_ids` is omitted
//! — EmbeddingGemma doesn't use it; BERT-style models tolerate its absence
//! when exported with the input as optional. If a model export hard-requires
//! it, swap the `feed_inputs` impl.

use anyhow::{Context, Result};
use ndarray::{Array2, Axis};
use ort::session::Session;
use ort::value::TensorRef;
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;
use tokio::sync::OnceCell;

use super::model_cache::embedder_paths;
use super::ort_runtime::build_session;

const EMBED_DIM: usize = 768;
const MAX_TOKENS: usize = 2048;

/// Output names we'll try, in order. First match wins.
const POOLED_OUTPUT_NAMES: &[&str] = &["sentence_embedding", "pooler_output"];
const HIDDEN_OUTPUT_NAMES: &[&str] = &["last_hidden_state", "token_embeddings"];

pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
}

/// Local embedder using direct ORT + HF tokenizers (nomic-embed-text-v1.5 int8).
///
/// Session is `Send + Sync` but `Session::run` takes `&mut self` for input
/// binding ergonomics in ort 2.x; we serialize calls through a Mutex. Inference
/// is invoked from `spawn_blocking` so the lock never crosses an await point.
pub struct LocalEmbedder {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
}

impl LocalEmbedder {
    pub async fn new() -> Result<Self> {
        let paths = embedder_paths().await?;
        let onnx = paths.onnx.clone();
        let tok_path = paths.tokenizer.clone();
        tokio::task::spawn_blocking(move || -> Result<Self> {
            let session = build_session(&onnx)?;
            let mut tokenizer = Tokenizer::from_file(&tok_path)
                .map_err(|e| anyhow::anyhow!("load tokenizer {}: {e}", tok_path.display()))?;
            // Pad to longest in batch — keeps tensors tight for short queries.
            if let Some(p) = tokenizer.get_padding_mut() {
                p.strategy = tokenizers::PaddingStrategy::BatchLongest;
            }
            Ok(Self { session: Mutex::new(session), tokenizer })
        })
        .await
        .map_err(|e| anyhow::anyhow!("embedder init panicked: {e}"))?
    }

    pub async fn embed_async(self: &Arc<Self>, text: &str) -> Result<Vec<f32>> {
        let this = self.clone();
        let text = text.to_string();
        tokio::task::spawn_blocking(move || this.embed(&text))
            .await
            .map_err(|e| anyhow::anyhow!("Embedding task panicked: {}", e))?
    }

    pub async fn embed_batch_async(self: &Arc<Self>, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            this.embed_batch(&refs)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Batch embedding task panicked: {}", e))?
    }
}

impl Embedder for LocalEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut out = self.embed_batch(&[text])?;
        out.pop().ok_or_else(|| anyhow::anyhow!("no embedding returned"))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let encodings = self
            .tokenizer
            .encode_batch(texts.iter().map(|s| s.to_string()).collect::<Vec<_>>(), true)
            .map_err(|e| anyhow::anyhow!("tokenize: {e}"))?;

        // Clamp to model's max context.
        let seq_len = encodings.iter().map(|e| e.len()).max().unwrap_or(0).min(MAX_TOKENS);
        let batch = encodings.len();

        let mut input_ids = Array2::<i64>::zeros((batch, seq_len));
        let mut attention_mask = Array2::<i64>::zeros((batch, seq_len));
        for (i, enc) in encodings.iter().enumerate() {
            for (j, (&id, &mask)) in enc.get_ids().iter().zip(enc.get_attention_mask()).enumerate() {
                if j >= seq_len {
                    break;
                }
                input_ids[[i, j]] = id as i64;
                attention_mask[[i, j]] = mask as i64;
            }
        }

        let mut session = self
            .session
            .lock()
            .map_err(|e| anyhow::anyhow!("embedder session poisoned: {e}"))?;

        // Build the input bind list. Include `token_type_ids` only if the
        // model declares it; EmbeddingGemma doesn't, classic BERT exports do.
        let wants_token_type_ids = session
            .inputs()
            .iter()
            .any(|i| i.name() == "token_type_ids");
        let token_type_ids = if wants_token_type_ids {
            Some(Array2::<i64>::zeros((batch, seq_len)))
        } else {
            None
        };

        let outputs = if let Some(ref tti) = token_type_ids {
            session.run(ort::inputs![
                "input_ids" => TensorRef::from_array_view(&input_ids)?,
                "attention_mask" => TensorRef::from_array_view(&attention_mask)?,
                "token_type_ids" => TensorRef::from_array_view(tti)?,
            ])
        } else {
            session.run(ort::inputs![
                "input_ids" => TensorRef::from_array_view(&input_ids)?,
                "attention_mask" => TensorRef::from_array_view(&attention_mask)?,
            ])
        }
        .context("embedder forward pass")?;

        // Prefer a pre-pooled output if the export provides one.
        for name in POOLED_OUTPUT_NAMES {
            if let Some(out) = outputs.get(*name) {
                let (shape, data) = out
                    .try_extract_tensor::<f32>()
                    .with_context(|| format!("extract pooled output `{name}`"))?;
                anyhow::ensure!(
                    shape.len() == 2,
                    "pooled output `{name}` has unexpected rank {}",
                    shape.len()
                );
                let d = shape[1] as usize;
                anyhow::ensure!(d == EMBED_DIM, "pooled output dim {d}, want {EMBED_DIM}");
                return Ok(l2_normalize_rows(data, batch, d));
            }
        }

        // Fall back to last_hidden_state + mask-aware mean pool + L2 normalize.
        let hidden_out = HIDDEN_OUTPUT_NAMES
            .iter()
            .find_map(|n| outputs.get(*n))
            .with_context(|| {
                format!(
                    "embedder ONNX has none of pooled outputs {:?} or hidden outputs {:?}",
                    POOLED_OUTPUT_NAMES, HIDDEN_OUTPUT_NAMES
                )
            })?;

        let (shape, data) = hidden_out
            .try_extract_tensor::<f32>()
            .context("extract hidden-state output tensor")?;
        anyhow::ensure!(shape.len() == 3, "hidden output rank {}", shape.len());
        let t = shape[1] as usize;
        let d = shape[2] as usize;
        anyhow::ensure!(d == EMBED_DIM, "hidden dim {d}, want {EMBED_DIM}");

        let hidden = ndarray::ArrayView3::from_shape((batch, t, d), data)
            .context("reshape hidden state")?;
        let mask_f = attention_mask.mapv(|v| v as f32);

        let mut results = Vec::with_capacity(batch);
        for b in 0..batch {
            let mask_row = mask_f.index_axis(Axis(0), b);
            let denom: f32 = mask_row.sum().max(1e-9);
            let hidden_row = hidden.index_axis(Axis(0), b);
            let mut pooled = vec![0f32; d];
            for tok in 0..t {
                let m = mask_row[tok];
                if m == 0.0 {
                    continue;
                }
                for k in 0..d {
                    pooled[k] += hidden_row[[tok, k]] * m;
                }
            }
            let mut norm_sq = 0f32;
            for k in 0..d {
                pooled[k] /= denom;
                norm_sq += pooled[k] * pooled[k];
            }
            let norm = norm_sq.sqrt().max(1e-12);
            for v in &mut pooled {
                *v /= norm;
            }
            results.push(pooled);
        }
        Ok(results)
    }

    fn dimension(&self) -> usize {
        EMBED_DIM
    }
}

/// L2-normalize a flat batch of vectors stored row-major as `batch × dim`.
fn l2_normalize_rows(data: &[f32], batch: usize, dim: usize) -> Vec<Vec<f32>> {
    let mut out = Vec::with_capacity(batch);
    for b in 0..batch {
        let row = &data[b * dim..(b + 1) * dim];
        let mut v: Vec<f32> = row.to_vec();
        let norm_sq: f32 = v.iter().map(|x| x * x).sum();
        let norm = norm_sq.sqrt().max(1e-12);
        for x in &mut v {
            *x /= norm;
        }
        out.push(v);
    }
    out
}

static EMBEDDER: OnceCell<Arc<LocalEmbedder>> = OnceCell::const_new();

pub async fn get_embedder() -> Result<Arc<LocalEmbedder>> {
    let embedder = EMBEDDER
        .get_or_try_init(|| async {
            tracing::info!("Loading local embedding model (embeddinggemma-300m int8)...");
            let start = std::time::Instant::now();
            let embedder = LocalEmbedder::new().await?;
            tracing::info!(
                "Embedding model loaded in {:.1}s (dim={})",
                start.elapsed().as_secs_f64(),
                embedder.dimension()
            );
            Ok::<_, anyhow::Error>(Arc::new(embedder))
        })
        .await?;
    Ok(embedder.clone())
}
