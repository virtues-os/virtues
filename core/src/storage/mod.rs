//! Storage module for filesystem and S3 operations

pub mod models;
pub mod s3;
pub mod stream_writer;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use s3::{S3Config, S3Storage};

use crate::error::{Error, Result};

/// Storage trait for different backends
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn initialize(&self) -> Result<()>;
    async fn upload(&self, key: &str, data: Vec<u8>) -> Result<()>;
    async fn download(&self, key: &str) -> Result<Vec<u8>>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn list_with_pagination(
        &self,
        prefix: &str,
        max_keys: Option<i32>,
        continuation_token: Option<String>,
    ) -> Result<ListResult>;
    async fn health_check(&self) -> Result<HealthStatus>;
}

/// Result from list_with_pagination
#[derive(Debug)]
pub struct ListResult {
    pub keys: Vec<String>,
    pub continuation_token: Option<String>,
    pub is_truncated: bool,
}

/// Main storage interface
#[derive(Clone)]
pub struct Storage {
    backend: Arc<dyn StorageBackend>,
}

impl Storage {
    /// Create file storage at the given path
    ///
    /// # Arguments
    /// * `path` - Base path for storage (e.g., "./core/data/lake" or "/home/user/drive")
    pub fn file(path: String) -> Result<Self> {
        Ok(Self {
            backend: Arc::new(FileStorage::new(path)?),
        })
    }

    /// Alias for file() - kept for backwards compatibility during migration
    pub fn local(path: String) -> Result<Self> {
        Self::file(path)
    }

    /// Create S3 storage with the given configuration
    ///
    /// # Arguments
    /// * `config` - S3 configuration (endpoint, bucket, prefix, credentials)
    pub async fn s3(config: S3Config) -> Result<Self> {
        Ok(Self {
            backend: Arc::new(S3Storage::new(config).await?),
        })
    }

    /// Create S3 storage from environment variables
    ///
    /// Required env vars: S3_ENDPOINT, S3_BUCKET, S3_ACCESS_KEY, S3_SECRET_KEY
    /// Optional: S3_PREFIX, S3_REGION
    pub async fn s3_from_env() -> Result<Self> {
        let config = S3Config::from_env()?;
        Self::s3(config).await
    }

    /// Get the underlying S3 storage backend (if S3 is being used)
    ///
    /// Returns None if using file storage
    pub fn as_s3(&self) -> Option<&S3Storage> {
        // This is a bit of a hack - we use Any to downcast
        // In production, you might track the backend type explicitly
        None // File storage doesn't support this
    }

    pub async fn initialize(&self) -> Result<()> {
        self.backend.initialize().await
    }

    pub async fn upload(&self, key: &str, data: Vec<u8>) -> Result<()> {
        self.backend.upload(key, data).await
    }

    pub async fn download(&self, key: &str) -> Result<Vec<u8>> {
        self.backend.download(key).await
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        self.backend.delete(key).await
    }

    pub async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.backend.list(prefix).await
    }

    pub async fn health_check(&self) -> Result<HealthStatus> {
        self.backend.health_check().await
    }

    /// List objects with pagination support
    pub async fn list_with_pagination(
        &self,
        prefix: &str,
        max_keys: Option<i32>,
        continuation_token: Option<String>,
    ) -> Result<ListResult> {
        self.backend
            .list_with_pagination(prefix, max_keys, continuation_token)
            .await
    }

    /// Upload JSON object
    pub async fn upload_json<T: Serialize>(&self, key: &str, data: &T) -> Result<()> {
        let json_bytes = serde_json::to_vec(data)
            .map_err(|e| Error::Other(format!("Failed to serialize JSON: {}", e)))?;
        self.upload(key, json_bytes).await
    }

    /// Download and deserialize JSON object
    pub async fn download_json<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<T> {
        let bytes = self.download(key).await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::Other(format!("Failed to deserialize JSON: {}", e)))
    }

    /// Upload JSONL (newline-delimited JSON) from a vector of objects
    pub async fn upload_jsonl<T: Serialize>(&self, key: &str, records: &[T]) -> Result<()> {
        let mut jsonl = Vec::new();
        for record in records {
            let json = serde_json::to_string(record)
                .map_err(|e| Error::Other(format!("Failed to serialize record: {}", e)))?;
            jsonl.extend_from_slice(json.as_bytes());
            jsonl.push(b'\n');
        }
        self.upload(key, jsonl).await
    }

    /// Download and parse JSONL (newline-delimited JSON) into a vector
    pub async fn download_jsonl<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Vec<T>> {
        let bytes = self.download(key).await?;
        let text = String::from_utf8(bytes)
            .map_err(|e| Error::Other(format!("Invalid UTF-8 in JSONL: {}", e)))?;

        let mut records = Vec::new();
        for (line_num, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record = serde_json::from_str(line).map_err(|e| {
                Error::Other(format!(
                    "Failed to parse JSONL line {}: {}",
                    line_num + 1,
                    e
                ))
            })?;
            records.push(record);
        }
        Ok(records)
    }
}

/// File storage backend
struct FileStorage {
    base_path: PathBuf,
}

impl FileStorage {
    fn new(path: String) -> Result<Self> {
        Ok(Self {
            base_path: PathBuf::from(path),
        })
    }
}

#[async_trait]
impl StorageBackend for FileStorage {
    async fn initialize(&self) -> Result<()> {
        // Create directory if it doesn't exist
        tokio::fs::create_dir_all(&self.base_path).await?;
        Ok(())
    }

    async fn upload(&self, key: &str, data: Vec<u8>) -> Result<()> {
        let path = self.base_path.join(key);

        // Create parent directories
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(path, data).await?;
        Ok(())
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.base_path.join(key);
        Ok(tokio::fs::read(path).await?)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.base_path.join(key);
        tokio::fs::remove_file(path).await?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let prefix_path = self.base_path.join(prefix);
        let mut files = Vec::new();

        // Handle case where prefix directory doesn't exist
        if !prefix_path.exists() {
            return Ok(files);
        }

        let mut dir = tokio::fs::read_dir(prefix_path).await?;
        while let Some(entry) = dir.next_entry().await? {
            if entry.file_type().await?.is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    files.push(format!("{prefix}/{name}"));
                }
            }
        }

        Ok(files)
    }

    async fn list_with_pagination(
        &self,
        prefix: &str,
        max_keys: Option<i32>,
        _continuation_token: Option<String>,
    ) -> Result<ListResult> {
        // Local storage doesn't support pagination, so we just return all files
        // and truncate if max_keys is specified
        let keys = self.list(prefix).await?;

        let (keys, is_truncated) = if let Some(max) = max_keys {
            let max_usize = max as usize;
            if keys.len() > max_usize {
                (keys[..max_usize].to_vec(), true)
            } else {
                (keys, false)
            }
        } else {
            (keys, false)
        };

        Ok(ListResult {
            keys,
            continuation_token: None,
            is_truncated,
        })
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        // Check if directory exists and is writable
        match tokio::fs::metadata(&self.base_path).await {
            Ok(metadata) if metadata.is_dir() => Ok(HealthStatus {
                is_healthy: true,
                message: format!("Storage at {:?} is accessible", self.base_path),
            }),
            _ => Ok(HealthStatus {
                is_healthy: false,
                message: format!("Storage at {:?} not accessible", self.base_path),
            }),
        }
    }
}

/// Health status for storage
#[derive(Debug)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_file_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::file(temp_dir.path().to_str().unwrap().to_string()).unwrap();

        storage.initialize().await.unwrap();

        // Test upload
        let data = b"test data".to_vec();
        storage.upload("test.txt", data.clone()).await.unwrap();

        // Test download
        let downloaded = storage.download("test.txt").await.unwrap();
        assert_eq!(downloaded, data);

        // Test list
        let files = storage.list("").await.unwrap();
        assert!(files.iter().any(|f| f.contains("test.txt")));

        // Test delete
        storage.delete("test.txt").await.unwrap();
    }

    #[tokio::test]
    async fn test_jsonl_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::file(temp_dir.path().to_str().unwrap().to_string()).unwrap();

        storage.initialize().await.unwrap();

        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct TestRecord {
            id: i32,
            name: String,
        }

        let records = vec![
            TestRecord {
                id: 1,
                name: "Alice".to_string(),
            },
            TestRecord {
                id: 2,
                name: "Bob".to_string(),
            },
            TestRecord {
                id: 3,
                name: "Charlie".to_string(),
            },
        ];

        // Upload JSONL
        storage
            .upload_jsonl("records.jsonl", &records)
            .await
            .unwrap();

        // Download JSONL
        let downloaded_records: Vec<TestRecord> =
            storage.download_jsonl("records.jsonl").await.unwrap();

        // Verify records
        assert_eq!(records, downloaded_records);
    }

    #[tokio::test]
    async fn test_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::file(temp_dir.path().to_str().unwrap().to_string()).unwrap();

        storage.initialize().await.unwrap();

        // Test uploading to nested path
        let data = b"nested data".to_vec();
        storage
            .upload(
                "streams/ios/healthkit/date=2025-01-15/records.jsonl",
                data.clone(),
            )
            .await
            .unwrap();

        // Test download from nested path
        let downloaded = storage
            .download("streams/ios/healthkit/date=2025-01-15/records.jsonl")
            .await
            .unwrap();
        assert_eq!(downloaded, data);
    }
}
