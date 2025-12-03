//! Ollama embedding client
//!
//! HTTP client for generating embeddings via local Ollama instance.

use crate::error::{Error, Result};
use ollama_rs::generation::embeddings::request::GenerateEmbeddingsRequest;
use ollama_rs::Ollama;

/// Client for generating embeddings via Ollama
pub struct EmbeddingClient {
    ollama: Ollama,
    model: String,
}

impl EmbeddingClient {
    /// Create a new embedding client
    ///
    /// # Arguments
    /// * `host` - Ollama host (e.g., "http://localhost" or "http://ollama")
    /// * `port` - Ollama port (default: 11434)
    /// * `model` - Embedding model name (e.g., "nomic-embed-text")
    pub fn new(host: &str, port: u16, model: String) -> Self {
        Self {
            ollama: Ollama::new(host.to_string(), port),
            model,
        }
    }

    /// Create client from environment variables
    ///
    /// Reads:
    /// - `OLLAMA_ENDPOINT` (default: "http://localhost:11434")
    /// - `OLLAMA_EMBEDDING_MODEL` (default: "nomic-embed-text")
    pub fn from_env() -> Result<Self> {
        let endpoint = std::env::var("OLLAMA_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        // Parse host and port from endpoint URL
        let url = url::Url::parse(&endpoint)
            .map_err(|e| Error::Other(format!("Invalid OLLAMA_ENDPOINT URL: {}", e)))?;

        let host = format!(
            "{}://{}",
            url.scheme(),
            url.host_str().unwrap_or("localhost")
        );
        let port = url.port().unwrap_or(11434);

        let model = std::env::var("OLLAMA_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "nomic-embed-text".to_string());

        Ok(Self::new(&host, port, model))
    }

    /// Create client with specific endpoint and model
    pub fn with_config(endpoint: &str, model: &str) -> Result<Self> {
        let url = url::Url::parse(endpoint)
            .map_err(|e| Error::Other(format!("Invalid endpoint URL: {}", e)))?;

        let host = format!(
            "{}://{}",
            url.scheme(),
            url.host_str().unwrap_or("localhost")
        );
        let port = url.port().unwrap_or(11434);

        Ok(Self::new(&host, port, model.to_string()))
    }

    /// Get the model name
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Generate embedding for a single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f64>> {
        let request = GenerateEmbeddingsRequest::new(self.model.clone(), text.into());

        let response = self
            .ollama
            .generate_embeddings(request)
            .await
            .map_err(|e| Error::Other(format!("Ollama embedding error: {}", e)))?;

        // ollama-rs returns Vec<Vec<f32>>, we need the first embedding as Vec<f64>
        let embedding = response
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| Error::Other("No embedding returned from Ollama".to_string()))?;

        // Convert f32 to f64 for higher precision storage
        Ok(embedding.into_iter().map(|x| x as f64).collect())
    }

    /// Generate embeddings for multiple texts
    ///
    /// Processes texts sequentially (Ollama doesn't have native batch API).
    /// For better performance with many texts, consider using `embed_batch_parallel`.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f64>>> {
        let mut embeddings = Vec::with_capacity(texts.len());

        for text in texts {
            let embedding = self.embed(text).await?;
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }

    /// Generate embeddings in parallel (limited concurrency)
    ///
    /// Uses tokio::join to process multiple embeddings concurrently.
    /// Limited to batches of 10 to avoid overwhelming Ollama.
    pub async fn embed_batch_parallel(&self, texts: &[String]) -> Result<Vec<Vec<f64>>> {
        use futures::future::join_all;

        let mut all_embeddings = Vec::with_capacity(texts.len());

        // Process in chunks of 10 for controlled parallelism
        for chunk in texts.chunks(10) {
            let futures: Vec<_> = chunk.iter().map(|text| self.embed(text)).collect();

            let results = join_all(futures).await;

            for result in results {
                all_embeddings.push(result?);
            }
        }

        Ok(all_embeddings)
    }

    /// Check if Ollama is available and the model is loaded
    pub async fn health_check(&self) -> Result<bool> {
        // Try to embed a simple text to verify connectivity
        match self.embed("test").await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!("Ollama health check failed: {}", e);
                Ok(false)
            }
        }
    }
}

/// Format embedding vector for PostgreSQL pgvector insertion
///
/// Converts Vec<f64> to string like "[0.1, 0.2, 0.3, ...]"
pub fn format_embedding_for_pg(embedding: &[f64]) -> String {
    format!(
        "[{}]",
        embedding
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_embedding() {
        let embedding = vec![0.1, 0.2, 0.3];
        let formatted = format_embedding_for_pg(&embedding);
        assert_eq!(formatted, "[0.1,0.2,0.3]");
    }

    #[test]
    fn test_client_creation() {
        let client = EmbeddingClient::new("http://localhost", 11434, "nomic-embed-text".to_string());
        assert_eq!(client.model(), "nomic-embed-text");
    }
}
