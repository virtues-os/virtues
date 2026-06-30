//! Newline-delimited-JSON framing over the box→relay connections.
//!
//! Mirrors the relay's framing (the shared *types* live in `virtues-protocol`;
//! the I/O lives in each consumer since `virtues-protocol` is pure/no-tokio).
//! The hello line is read byte-by-byte so a work connection's post-hello bytes
//! stay intact for splicing.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_LINE: usize = 8 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("connection closed before a full line")]
    Eof,
    #[error("line exceeded {MAX_LINE} bytes")]
    TooLong,
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub async fn read_line<R>(r: &mut R) -> Result<String, WireError>
where
    R: AsyncRead + Unpin,
{
    let mut buf = Vec::with_capacity(128);
    let mut byte = [0u8; 1];
    loop {
        let n = r.read(&mut byte).await?;
        if n == 0 {
            return Err(WireError::Eof);
        }
        if byte[0] == b'\n' {
            break;
        }
        buf.push(byte[0]);
        if buf.len() > MAX_LINE {
            return Err(WireError::TooLong);
        }
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

pub async fn read_msg<R, T>(r: &mut R) -> Result<T, WireError>
where
    R: AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let line = read_line(r).await?;
    Ok(serde_json::from_str(&line)?)
}

pub async fn write_msg<W, T>(w: &mut W, msg: &T) -> Result<(), WireError>
where
    W: AsyncWrite + Unpin,
    T: serde::Serialize,
{
    let mut line = serde_json::to_string(msg)?;
    line.push('\n');
    w.write_all(line.as_bytes()).await?;
    w.flush().await?;
    Ok(())
}
