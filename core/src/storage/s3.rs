//! S3 storage backend for cloud object storage
//!
//! Supports any S3-compatible service (AWS S3, Hetzner Object Storage, MinIO, etc.)

use async_trait::async_trait;
use aws_sdk_s3::{
    config::{Credentials, Region},
    primitives::ByteStream,
    Client, Config,
};
use tokio::io::AsyncRead;
use tokio_util::io::ReaderStream;

use super::{HealthStatus, ListResult, StorageBackend};
use crate::error::{Error, Result};

/// S3 storage backend configuration
#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub prefix: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
}

impl S3Config {
    /// Load S3 configuration from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            endpoint: std::env::var("S3_ENDPOINT")
                .map_err(|_| Error::Configuration("S3_ENDPOINT not set".into()))?,
            bucket: std::env::var("S3_BUCKET")
                .map_err(|_| Error::Configuration("S3_BUCKET not set".into()))?,
            prefix: std::env::var("S3_PREFIX").unwrap_or_default(),
            access_key: std::env::var("S3_ACCESS_KEY")
                .map_err(|_| Error::Configuration("S3_ACCESS_KEY not set".into()))?,
            secret_key: std::env::var("S3_SECRET_KEY")
                .map_err(|_| Error::Configuration("S3_SECRET_KEY not set".into()))?,
            region: std::env::var("S3_REGION").unwrap_or_else(|_| "auto".to_string()),
        })
    }

    /// Check if S3 is configured via environment variables
    pub fn is_configured() -> bool {
        std::env::var("S3_ENDPOINT").is_ok()
            && std::env::var("S3_BUCKET").is_ok()
            && std::env::var("S3_ACCESS_KEY").is_ok()
            && std::env::var("S3_SECRET_KEY").is_ok()
    }
}

/// S3 storage backend
pub struct S3Storage {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3Storage {
    /// Create a new S3 storage backend
    pub async fn new(config: S3Config) -> Result<Self> {
        let credentials = Credentials::new(
            &config.access_key,
            &config.secret_key,
            None, // session token
            None, // expiration
            "virtues-s3",
        );

        let s3_config = Config::builder()
            .endpoint_url(&config.endpoint)
            .credentials_provider(credentials)
            .region(Region::new(config.region))
            .force_path_style(true) // Required for non-AWS S3 endpoints
            .build();

        let client = Client::from_conf(s3_config);

        Ok(Self {
            client,
            bucket: config.bucket,
            prefix: config.prefix,
        })
    }

    /// Build full S3 key with prefix
    fn full_key(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", self.prefix.trim_end_matches('/'), key)
        }
    }

    /// Strip prefix from S3 key
    fn strip_prefix(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.to_string()
        } else {
            let prefix_with_slash = format!("{}/", self.prefix.trim_end_matches('/'));
            key.strip_prefix(&prefix_with_slash)
                .unwrap_or(key)
                .to_string()
        }
    }

    /// Download file as a byte stream (for large files)
    ///
    /// Returns an AsyncRead that can be wrapped with ReaderStream for streaming responses
    pub async fn download_reader(&self, key: &str) -> Result<impl AsyncRead + Send + Unpin> {
        let full_key = self.full_key(key);

        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&full_key)
            .send()
            .await
            .map_err(|e| Error::Storage(format!("Failed to download from S3: {}", e)))?;

        // Convert ByteStream to AsyncRead
        Ok(response.body.into_async_read())
    }

    /// Download file as a byte stream (for large files)
    ///
    /// Returns a Stream of Bytes for streaming HTTP responses
    pub async fn download_stream(
        &self,
        key: &str,
    ) -> Result<ReaderStream<impl AsyncRead + Send + Unpin>> {
        let reader = self.download_reader(key).await?;
        Ok(ReaderStream::new(reader))
    }

    /// Check if an object exists
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let full_key = self.full_key(key);

        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&full_key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                // Check if it's a "not found" error
                let service_error = e.into_service_error();
                if service_error.is_not_found() {
                    Ok(false)
                } else {
                    Err(Error::Storage(format!(
                        "Failed to check S3 object: {}",
                        service_error
                    )))
                }
            }
        }
    }

    /// Copy an object within S3
    pub async fn copy(&self, source_key: &str, dest_key: &str) -> Result<()> {
        let source_full_key = self.full_key(source_key);
        let dest_full_key = self.full_key(dest_key);

        self.client
            .copy_object()
            .bucket(&self.bucket)
            .copy_source(format!("{}/{}", self.bucket, source_full_key))
            .key(&dest_full_key)
            .send()
            .await
            .map_err(|e| Error::Storage(format!("Failed to copy S3 object: {}", e)))?;

        Ok(())
    }

    /// Delete all objects with a given prefix (for folder deletion)
    pub async fn delete_prefix(&self, prefix: &str) -> Result<u64> {
        let full_prefix = self.full_key(prefix);
        let mut deleted_count = 0u64;
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&full_prefix);

            if let Some(token) = continuation_token.take() {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|e| Error::Storage(format!("Failed to list S3 objects: {}", e)))?;

            // Delete objects in this batch
            if let Some(contents) = response.contents {
                for object in contents {
                    if let Some(key) = object.key {
                        self.client
                            .delete_object()
                            .bucket(&self.bucket)
                            .key(&key)
                            .send()
                            .await
                            .map_err(|e| {
                                Error::Storage(format!("Failed to delete S3 object: {}", e))
                            })?;
                        deleted_count += 1;
                    }
                }
            }

            // Check if there are more objects
            if response.is_truncated.unwrap_or(false) {
                continuation_token = response.next_continuation_token;
            } else {
                break;
            }
        }

        Ok(deleted_count)
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    async fn initialize(&self) -> Result<()> {
        // Verify bucket access by attempting a head bucket operation
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| Error::Storage(format!("Failed to access S3 bucket: {}", e)))?;

        tracing::info!(
            bucket = %self.bucket,
            prefix = %self.prefix,
            "S3 storage initialized"
        );

        Ok(())
    }

    async fn upload(&self, key: &str, data: Vec<u8>) -> Result<()> {
        let full_key = self.full_key(key);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&full_key)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| Error::Storage(format!("Failed to upload to S3: {}", e)))?;

        Ok(())
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>> {
        let full_key = self.full_key(key);

        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&full_key)
            .send()
            .await
            .map_err(|e| Error::Storage(format!("Failed to download from S3: {}", e)))?;

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| Error::Storage(format!("Failed to read S3 response body: {}", e)))?
            .into_bytes();

        Ok(bytes.to_vec())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let full_key = self.full_key(key);

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&full_key)
            .send()
            .await
            .map_err(|e| Error::Storage(format!("Failed to delete from S3: {}", e)))?;

        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let full_prefix = self.full_key(prefix);
        let mut keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&full_prefix);

            if let Some(token) = continuation_token.take() {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|e| Error::Storage(format!("Failed to list S3 objects: {}", e)))?;

            if let Some(contents) = response.contents {
                for object in contents {
                    if let Some(key) = object.key {
                        keys.push(self.strip_prefix(&key));
                    }
                }
            }

            if response.is_truncated.unwrap_or(false) {
                continuation_token = response.next_continuation_token;
            } else {
                break;
            }
        }

        Ok(keys)
    }

    async fn list_with_pagination(
        &self,
        prefix: &str,
        max_keys: Option<i32>,
        continuation_token: Option<String>,
    ) -> Result<ListResult> {
        let full_prefix = self.full_key(prefix);

        let mut request = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&full_prefix);

        if let Some(max) = max_keys {
            request = request.max_keys(max);
        }

        if let Some(token) = continuation_token {
            request = request.continuation_token(token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::Storage(format!("Failed to list S3 objects: {}", e)))?;

        let keys = response
            .contents
            .unwrap_or_default()
            .into_iter()
            .filter_map(|obj| obj.key.map(|k| self.strip_prefix(&k)))
            .collect();

        Ok(ListResult {
            keys,
            continuation_token: response.next_continuation_token,
            is_truncated: response.is_truncated.unwrap_or(false),
        })
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        match self.client.head_bucket().bucket(&self.bucket).send().await {
            Ok(_) => Ok(HealthStatus {
                is_healthy: true,
                message: format!("S3 bucket '{}' is accessible", self.bucket),
            }),
            Err(e) => Ok(HealthStatus {
                is_healthy: false,
                message: format!("S3 bucket '{}' not accessible: {}", self.bucket, e),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_key_with_prefix() {
        // Simulate S3Storage without actually connecting
        let storage = S3Storage {
            client: unsafe { std::mem::zeroed() }, // Never called in these tests
            bucket: "test".to_string(),
            prefix: "users/acme".to_string(),
        };

        assert_eq!(
            storage.full_key("drive/file.txt"),
            "users/acme/drive/file.txt"
        );
        assert_eq!(
            storage.strip_prefix("users/acme/drive/file.txt"),
            "drive/file.txt"
        );
    }

    #[test]
    fn test_full_key_without_prefix() {
        let storage = S3Storage {
            client: unsafe { std::mem::zeroed() },
            bucket: "test".to_string(),
            prefix: "".to_string(),
        };

        assert_eq!(storage.full_key("drive/file.txt"), "drive/file.txt");
        assert_eq!(storage.strip_prefix("drive/file.txt"), "drive/file.txt");
    }

    #[test]
    fn test_s3_config_is_configured() {
        // Without env vars set, should return false
        assert!(!S3Config::is_configured());
    }
}
