//! Cross-encoder reranker via direct ORT + tokenizers.
//!
//! Model: jinaai/jina-reranker-v2-base-multilingual, pre-quantized int8 ONNX.
//! ~280MB on disk; ~half the params of bge-reranker-v2-m3 with comparable
//! quality and a published int8 build (no runtime quantization).

use anyhow::{Context, Result};
use ndarray::Array2;
use ort::session::Session;
use ort::value::TensorRef;
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;
use tokio::sync::OnceCell;

use super::model_cache::reranker_paths;
use super::ort_runtime::build_session;

const MAX_TOKENS: usize = 1024;

pub struct LocalReranker {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
}

/// Score from the cross-encoder reranker.
#[derive(Debug, Clone)]
pub struct RerankScore {
    pub index: usize,
    pub score: f32,
}

impl LocalReranker {
    pub async fn new() -> Result<Self> {
        let paths = reranker_paths().await?;
        let onnx = paths.onnx.clone();
        let tok_path = paths.tokenizer.clone();
        tokio::task::spawn_blocking(move || -> Result<Self> {
            let session = build_session(&onnx)?;
            let mut tokenizer = Tokenizer::from_file(&tok_path)
                .map_err(|e| anyhow::anyhow!("load tokenizer {}: {e}", tok_path.display()))?;
            if let Some(p) = tokenizer.get_padding_mut() {
                p.strategy = tokenizers::PaddingStrategy::BatchLongest;
            }
            Ok(Self { session: Mutex::new(session), tokenizer })
        })
        .await
        .map_err(|e| anyhow::anyhow!("reranker init panicked: {e}"))?
    }

    /// Score `documents` against `query`. Returned vector is in the input
    /// order — callers sort if they want a ranked list.
    pub fn rerank(&self, query: &str, documents: &[&str]) -> Result<Vec<RerankScore>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let pairs: Vec<(String, String)> = documents
            .iter()
            .map(|d| (query.to_string(), d.to_string()))
            .collect();

        let encodings = self
            .tokenizer
            .encode_batch(pairs, true)
            .map_err(|e| anyhow::anyhow!("tokenize pairs: {e}"))?;

        let seq_len = encodings.iter().map(|e| e.len()).max().unwrap_or(0).min(MAX_TOKENS);
        let batch = encodings.len();

        let mut input_ids = Array2::<i64>::zeros((batch, seq_len));
        let mut attention_mask = Array2::<i64>::zeros((batch, seq_len));
        let mut token_type_ids = Array2::<i64>::zeros((batch, seq_len));
        for (i, enc) in encodings.iter().enumerate() {
            for (j, ((&id, &mask), &ttid)) in enc
                .get_ids()
                .iter()
                .zip(enc.get_attention_mask())
                .zip(enc.get_type_ids())
                .enumerate()
            {
                if j >= seq_len {
                    break;
                }
                input_ids[[i, j]] = id as i64;
                attention_mask[[i, j]] = mask as i64;
                token_type_ids[[i, j]] = ttid as i64;
            }
        }

        let mut session = self
            .session
            .lock()
            .map_err(|e| anyhow::anyhow!("reranker session poisoned: {e}"))?;

        let outputs = session
            .run(ort::inputs![
                "input_ids" => TensorRef::from_array_view(&input_ids)?,
                "attention_mask" => TensorRef::from_array_view(&attention_mask)?,
                "token_type_ids" => TensorRef::from_array_view(&token_type_ids)?,
            ])
            .context("reranker forward pass")?;

        // jina reranker emits a (B, 1) logits tensor at output 0.
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .context("extract reranker output tensor")?;
        anyhow::ensure!(
            shape[0] as usize == batch,
            "reranker output batch {} != input batch {}",
            shape[0],
            batch
        );

        Ok((0..batch)
            .map(|i| RerankScore { index: i, score: data[i] })
            .collect())
    }

    pub async fn rerank_async(
        self: &Arc<Self>,
        query: &str,
        documents: &[String],
    ) -> Result<Vec<RerankScore>> {
        let this = self.clone();
        let query = query.to_string();
        let docs = documents.to_vec();
        tokio::task::spawn_blocking(move || {
            let doc_refs: Vec<&str> = docs.iter().map(|s| s.as_str()).collect();
            this.rerank(&query, &doc_refs)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Rerank task panicked: {}", e))?
    }
}

static RERANKER: OnceCell<Arc<LocalReranker>> = OnceCell::const_new();

pub async fn get_reranker() -> Result<Arc<LocalReranker>> {
    let reranker = RERANKER
        .get_or_try_init(|| async {
            tracing::info!("Loading cross-encoder reranker (jina-reranker-v2 int8)...");
            let start = std::time::Instant::now();
            let reranker = LocalReranker::new().await?;
            tracing::info!(
                "Reranker model loaded in {:.1}s",
                start.elapsed().as_secs_f64()
            );
            Ok::<_, anyhow::Error>(Arc::new(reranker))
        })
        .await?;
    Ok(reranker.clone())
}
