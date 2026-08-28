//! Storage module — local filesystem backend.

pub mod lake;
pub mod volumes;
pub mod models;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A byte stream plus the total size of the underlying object — what a ranged
/// or streaming HTTP response needs without ever materializing the object.
pub type ObjectStream = (u64, BoxStream<'static, std::io::Result<Bytes>>);

/// Resolve a storage key to a path inside the lake, refusing anything that
/// would escape it.
///
/// The key is not trusted. It is built as
/// `media/{provider}/{stream}/{stream_id}.{audio_format}` and BOTH of those
/// last two come verbatim out of a device's JSON payload. So a paired phone —
/// compromised, buggy, or malicious — POSTing
/// `{"id":"../../../virtues","audio_format":"env"}` produced the key
/// `media/ios/mic/../../../virtues.env`, and `base_path.join(key)` resolved it
/// to `/var/lib/virtues/virtues.env`. Overwriting that file destroys
/// VIRTUES_ENCRYPTION_KEY, at which point every encrypted column on the box is
/// permanently undecryptable — silently, because nothing reads the key until
/// something needs to decrypt.
///
/// Rejects on the COMPONENTS rather than on the string: a substring test for
/// `..` misses encodings and absolute paths, while `Component::Normal` admits
/// exactly "a plain name relative to here" and nothing else.
fn safe_join(base: &Path, key: &str) -> Result<PathBuf> {
    use std::path::Component;
    let rel = Path::new(key);
    if !rel
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(Error::Other(format!(
            "unsafe storage key {key:?}: keys must be plain relative paths"
        )));
    }
    Ok(base.join(rel))
}

/// Storage trait for different backends
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn initialize(&self) -> Result<()>;
    async fn upload(&self, key: &str, data: Vec<u8>) -> Result<()>;
    /// Move an already-materialized file into storage. Callers stage large
    /// uploads on the same filesystem (see `staging_dir`) so this is a rename,
    /// not a copy — the file is never held in memory.
    async fn upload_from_file(&self, key: &str, src: &Path) -> Result<()>;
    async fn download(&self, key: &str) -> Result<Vec<u8>>;
    /// Stream an object without loading it into memory. `range` is
    /// `(start, len)` in bytes, already validated against the object size.
    async fn read_stream(&self, key: &str, range: Option<(u64, u64)>) -> Result<ObjectStream>;
    /// Directory on the same filesystem as the store for staging streamed
    /// uploads, so `upload_from_file` can rename instead of copy.
    async fn staging_dir(&self) -> Result<PathBuf>;
    /// `(total, available)` bytes of the filesystem holding the store — the
    /// box's real capacity, which is what quota displays and free-space checks
    /// should reflect.
    async fn disk_stats(&self) -> Result<(u64, u64)>;
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

    pub async fn initialize(&self) -> Result<()> {
        self.backend.initialize().await
    }

    pub async fn upload(&self, key: &str, data: Vec<u8>) -> Result<()> {
        self.backend.upload(key, data).await
    }

    pub async fn upload_from_file(&self, key: &str, src: &Path) -> Result<()> {
        self.backend.upload_from_file(key, src).await
    }

    pub async fn download(&self, key: &str) -> Result<Vec<u8>> {
        self.backend.download(key).await
    }

    pub async fn read_stream(&self, key: &str, range: Option<(u64, u64)>) -> Result<ObjectStream> {
        self.backend.read_stream(key, range).await
    }

    pub async fn staging_dir(&self) -> Result<PathBuf> {
        self.backend.staging_dir().await
    }

    pub async fn disk_stats(&self) -> Result<(u64, u64)> {
        self.backend.disk_stats().await
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
        let path = safe_join(&self.base_path, key)?;

        // Create parent directories
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(path, data).await?;
        Ok(())
    }

    async fn upload_from_file(&self, key: &str, src: &Path) -> Result<()> {
        let path = safe_join(&self.base_path, key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        // Same-filesystem staging makes this a rename; fall back to copy if the
        // source landed on another device (EXDEV).
        if tokio::fs::rename(src, &path).await.is_err() {
            tokio::fs::copy(src, &path).await?;
            let _ = tokio::fs::remove_file(src).await;
        }
        Ok(())
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>> {
        let path = safe_join(&self.base_path, key)?;
        Ok(tokio::fs::read(path).await?)
    }

    async fn read_stream(&self, key: &str, range: Option<(u64, u64)>) -> Result<ObjectStream> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let path = safe_join(&self.base_path, key)?;
        let mut file = tokio::fs::File::open(path).await?;
        let total = file.metadata().await?.len();
        let stream: BoxStream<'static, std::io::Result<Bytes>> = match range {
            Some((start, len)) => {
                file.seek(std::io::SeekFrom::Start(start)).await?;
                Box::pin(tokio_util::io::ReaderStream::new(file.take(len)))
            }
            None => Box::pin(tokio_util::io::ReaderStream::new(file)),
        };
        Ok((total, stream))
    }

    async fn staging_dir(&self) -> Result<PathBuf> {
        // Hidden sibling of user content: same filesystem (rename-cheap), never
        // listed (listings come from the DB) and never counted (usage is
        // DB-tracked). Stale `.part` files from crashed uploads are harmless.
        let dir = self.base_path.join(".uploads");
        tokio::fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    async fn disk_stats(&self) -> Result<(u64, u64)> {
        // Resolve against the disk whose mount point is the longest prefix of
        // the store's path — that filesystem's capacity is the real quota.
        let base = tokio::fs::canonicalize(&self.base_path)
            .await
            .unwrap_or_else(|_| self.base_path.clone());
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let best = disks
            .iter()
            .filter(|d| base.starts_with(d.mount_point()))
            .max_by_key(|d| d.mount_point().as_os_str().len());
        match best {
            Some(d) => Ok((d.total_space(), d.available_space())),
            None => Err(Error::Storage(format!(
                "No mounted filesystem found for {:?}",
                base
            ))),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = safe_join(&self.base_path, key)?;
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
        // Actually WRITE. This said "exists and is writable" in a comment and
        // then only asked `is_dir()`, which is true of a directory owned by
        // root that this process cannot write a byte into — precisely the state
        // a box seeded from a card boots in. The lie was free until an applet
        // hit EACCES every five minutes for days with nothing else reporting a
        // problem.
        //
        // A probe file rather than a permissions calculation: mode bits, owner,
        // group membership, ACLs and read-only mounts all have to come out
        // right, and the only way to know they did is to try.
        match tokio::fs::metadata(&self.base_path).await {
            Ok(m) if m.is_dir() => {}
            _ => {
                return Ok(HealthStatus {
                    is_healthy: false,
                    message: format!("Storage at {:?} not accessible", self.base_path),
                })
            }
        }
        let probe = self.base_path.join(".virtues-write-probe");
        match tokio::fs::write(&probe, b"").await {
            Ok(()) => {
                let _ = tokio::fs::remove_file(&probe).await;
                Ok(HealthStatus {
                    is_healthy: true,
                    message: format!("Storage at {:?} is writable", self.base_path),
                })
            }
            Err(e) => Ok(HealthStatus {
                is_healthy: false,
                message: format!(
                    "Storage at {:?} is NOT writable ({e}). The service runs as `virtues`; \
                     fix with: sudo chown -R virtues:virtues {}",
                    self.base_path,
                    self.base_path.display()
                ),
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
    async fn test_staged_upload_and_ranged_stream() {
        use futures::StreamExt;

        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::file(temp_dir.path().to_str().unwrap().to_string()).unwrap();
        storage.initialize().await.unwrap();

        // Stage a file and move it in — must not remain in staging afterwards.
        let staging = storage.staging_dir().await.unwrap();
        let part = staging.join("ten.part");
        tokio::fs::write(&part, b"0123456789").await.unwrap();
        storage
            .upload_from_file("nested/dir/ten.bin", &part)
            .await
            .unwrap();
        assert!(!part.exists());

        async fn collect(stream: BoxStream<'static, std::io::Result<Bytes>>) -> Vec<u8> {
            stream
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .flat_map(|c| c.unwrap().to_vec())
                .collect()
        }

        let (total, stream) = storage.read_stream("nested/dir/ten.bin", None).await.unwrap();
        assert_eq!(total, 10);
        assert_eq!(collect(stream).await, b"0123456789");

        let (total, stream) = storage
            .read_stream("nested/dir/ten.bin", Some((2, 5)))
            .await
            .unwrap();
        assert_eq!(total, 10);
        assert_eq!(collect(stream).await, b"23456");
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

#[cfg(test)]
mod safe_join_tests {
    use super::safe_join;
    use std::path::Path;

    /// A device-supplied key must not be able to write outside the lake.
    ///
    /// The key is `media/{provider}/{stream}/{stream_id}.{audio_format}` and the
    /// last two come verbatim from a phone's JSON. `../../../virtues` + `env`
    /// resolved to `/var/lib/virtues/virtues.env` — overwriting the encryption
    /// key and making every encrypted column permanently undecryptable.
    #[test]
    fn traversal_keys_are_refused() {
        let base = Path::new("/var/lib/virtues/lake");
        for k in [
            "media/ios/mic/../../../virtues.env",
            "../virtues.env",
            "/etc/passwd",
            "media/ios/../../..",
            "./media/ios/x",
        ] {
            assert!(safe_join(base, k).is_err(), "must refuse {k}");
        }
    }

    #[test]
    fn ordinary_keys_still_resolve() {
        let base = Path::new("/var/lib/virtues/lake");
        let p = safe_join(base, "media/ios/microphone/abc123.m4a").expect("ordinary key");
        assert_eq!(p, base.join("media/ios/microphone/abc123.m4a"));
    }
}
