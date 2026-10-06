//! The door's one line back to the core: a Unix socket.
//!
//! The door holds no database access, so a live page's queries and the open
//! counts go through the core, which decides everything. The door only
//! forwards `(token, key)`; the core runs a query only if that key is one the
//! owner approved for that link, read-only, with a row cap
//! (`api::publications::answer`).
//!
//! One JSON line each way per connection:
//!
//! ```text
//! → {"op":"query","token":"…","key":"…"}   ← {"ok":true,"rows":[…]} | {"ok":false}
//! → {"op":"opened","token":"…"}            ← {"ok":true}
//! ```

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// The largest answer the door accepts from the core, and so the largest a
/// visitor can receive for one query.
pub const MAX_ANSWER_BYTES: u64 = 4 * 1024 * 1024;

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum CoreRequest {
    Query { token: String, key: String },
    Opened { token: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CoreAnswer {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<serde_json::Value>,
}

/// Where the core listens, or nothing (a door with no core answers no
/// queries and counts nothing, and still serves pages).
#[derive(Debug, Clone, Default)]
pub struct Core {
    path: Option<PathBuf>,
}

impl Core {
    pub fn at(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    async fn ask(&self, request: &CoreRequest) -> Option<CoreAnswer> {
        let path = self.path.as_ref()?;
        let exchange = async {
            let mut stream = UnixStream::connect(path).await.ok()?;
            let mut line = serde_json::to_vec(request).ok()?;
            line.push(b'\n');
            stream.write_all(&line).await.ok()?;
            let mut reply = String::new();
            BufReader::new(stream.take(MAX_ANSWER_BYTES + 1)).read_line(&mut reply).await.ok()?;
            serde_json::from_str::<CoreAnswer>(reply.trim_end()).ok()
        };
        tokio::time::timeout(TIMEOUT, exchange).await.ok().flatten()
    }

    /// The rows for an approved query, serialized, or `None` for anything
    /// else: an unapproved key, a dead link, no core, an error.
    pub async fn query(&self, token: &str, key: &str) -> Option<Vec<u8>> {
        let answer = self
            .ask(&CoreRequest::Query { token: token.to_string(), key: key.to_string() })
            .await?;
        if !answer.ok {
            return None;
        }
        serde_json::to_vec(&answer.rows?).ok()
    }

    /// Tell the core a page was served. Best effort, never waited on.
    pub fn opened(&self, token: &str) {
        let core = self.clone();
        let token = token.to_string();
        tokio::spawn(async move {
            let _ = core.ask(&CoreRequest::Opened { token }).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn a_query_goes_to_the_core_and_its_answer_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("c.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        tokio::spawn(async move {
            for _ in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let (read, mut write) = stream.into_split();
                let mut line = String::new();
                BufReader::new(read).read_line(&mut line).await.unwrap();
                let req: CoreRequest = serde_json::from_str(line.trim()).unwrap();
                let reply = match req {
                    CoreRequest::Query { key, .. } if key == "good" => r#"{"ok":true,"rows":[{"n":1}]}"#,
                    _ => r#"{"ok":false}"#,
                };
                write.write_all(format!("{reply}\n").as_bytes()).await.unwrap();
            }
        });
        let core = Core::at(Some(sock));
        assert_eq!(core.query("t", "good").await.as_deref(), Some(&br#"[{"n":1}]"#[..]));
        assert_eq!(core.query("t", "bad").await, None);
        assert_eq!(Core::default().query("t", "good").await, None, "no core, no rows");
    }
}
