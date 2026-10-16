//! JSONL framing shared by pi and omp.
//!
//! Records are split on LF only. Generic line readers that also split on
//! U+2028/U+2029 would corrupt valid JSON strings (see pi `rpc.md`).
//! omp protocol v2 may additionally wrap one oversized logical frame in a
//! sequence of `rpc_chunk` frames, which [`ChunkAssembler`] reassembles.

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// Upper bound for one physical line. omp advertises 1 MiB physical frames
/// and 64 MiB reassembled frames; pi has no limit, so allow the larger one.
pub const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;

/// Reads one LF-terminated record. Returns `None` at EOF.
/// A trailing CR is stripped so CRLF input is accepted.
pub async fn read_record<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> Result<Option<Vec<u8>>> {
    buf.clear();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if buf.is_empty() {
                return Ok(None);
            }
            // EOF without a trailing newline: deliver what we have.
            return Ok(Some(std::mem::take(buf)));
        }
        if let Some(pos) = available.iter().position(|b| *b == b'\n') {
            buf.extend_from_slice(&available[..pos]);
            reader.consume(pos + 1);
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
            return Ok(Some(std::mem::take(buf)));
        }
        let len = available.len();
        buf.extend_from_slice(available);
        reader.consume(len);
        if buf.len() > MAX_LINE_BYTES {
            bail!("protocol record exceeds {MAX_LINE_BYTES} bytes");
        }
    }
}

/// Reassembles omp `rpc_chunk` sequences into one logical JSON frame.
#[derive(Default)]
pub struct ChunkAssembler {
    current: Option<PendingChunk>,
}

struct PendingChunk {
    chunk_id: String,
    count: u64,
    next_index: u64,
    byte_length: u64,
    data: Vec<u8>,
}

impl ChunkAssembler {
    /// True while a chunk sequence is open. Any non-chunk frame arriving now
    /// means the sequence was interrupted.
    pub fn in_progress(&self) -> bool {
        self.current.is_some()
    }

    pub fn abandon(&mut self) {
        self.current = None;
    }

    /// Feeds one `rpc_chunk` frame. Returns the reassembled frame once the
    /// final chunk arrives.
    pub fn push(&mut self, frame: &Value) -> Result<Option<Value>> {
        let chunk_id = frame["chunkId"].as_str().context("rpc_chunk without chunkId")?;
        let index = frame["index"].as_u64().context("rpc_chunk without index")?;
        let count = frame["count"].as_u64().context("rpc_chunk without count")?;
        let byte_length = frame["byteLength"].as_u64().context("rpc_chunk without byteLength")?;
        let data = frame["data"].as_str().context("rpc_chunk without data")?;
        if count == 0 || index >= count {
            self.current = None;
            bail!("rpc_chunk {chunk_id}: index {index} out of range for count {count}");
        }
        if byte_length > MAX_LINE_BYTES as u64 {
            self.current = None;
            bail!("rpc_chunk {chunk_id}: byteLength {byte_length} exceeds limit");
        }

        if index == 0 {
            if self.current.is_some() {
                self.current = None;
                bail!("rpc_chunk {chunk_id} started before the previous sequence finished");
            }
            self.current = Some(PendingChunk {
                chunk_id: chunk_id.to_owned(),
                count,
                next_index: 0,
                byte_length,
                data: Vec::with_capacity(byte_length as usize),
            });
        }

        let pending = self
            .current
            .as_mut()
            .ok_or_else(|| anyhow!("rpc_chunk {chunk_id} index {index} without a start"))?;
        if pending.chunk_id != chunk_id
            || pending.count != count
            || pending.byte_length != byte_length
            || pending.next_index != index
        {
            self.current = None;
            bail!("rpc_chunk {chunk_id}: interleaved or out-of-order chunk {index}");
        }

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(data)
            .with_context(|| format!("rpc_chunk {chunk_id}: invalid base64"))?;
        pending.data.extend_from_slice(&decoded);
        pending.next_index += 1;
        if pending.data.len() as u64 > pending.byte_length {
            self.current = None;
            bail!("rpc_chunk {chunk_id}: more data than byteLength");
        }

        if pending.next_index < pending.count {
            return Ok(None);
        }
        let done = self.current.take().expect("pending chunk");
        if done.data.len() as u64 != done.byte_length {
            bail!(
                "rpc_chunk {}: got {} bytes, expected {}",
                done.chunk_id,
                done.data.len(),
                done.byte_length
            );
        }
        let text = String::from_utf8(done.data)
            .with_context(|| format!("rpc_chunk {}: invalid UTF-8", done.chunk_id))?;
        Ok(Some(serde_json::from_str(&text)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn splits_only_on_lf() {
        let input = "{\"a\":\"x\u{2028}y\"}\r\n{\"b\":1}\n{\"c\":2}".as_bytes();
        let mut reader = BufReader::with_capacity(4, input);
        let mut buf = Vec::new();
        let mut out = Vec::new();
        while let Some(rec) = read_record(&mut reader, &mut buf).await.unwrap() {
            out.push(String::from_utf8(rec).unwrap());
        }
        assert_eq!(out, ["{\"a\":\"x\u{2028}y\"}", "{\"b\":1}", "{\"c\":2}"]);
    }

    fn chunks(id: &str, payload: &str, size: usize) -> Vec<Value> {
        let bytes = payload.as_bytes();
        let parts: Vec<&[u8]> = bytes.chunks(size).collect();
        parts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                json!({
                    "type": "rpc_chunk",
                    "chunkId": id,
                    "index": i,
                    "count": parts.len(),
                    "byteLength": bytes.len(),
                    "data": base64::engine::general_purpose::STANDARD.encode(p),
                })
            })
            .collect()
    }

    #[test]
    fn reassembles_chunks() {
        let payload = r#"{"type":"message_end","message":{"role":"assistant","content":"héllo wörld"}}"#;
        let mut asm = ChunkAssembler::default();
        let frames = chunks("c1", payload, 7);
        let mut result = None;
        for f in &frames {
            result = asm.push(f).unwrap();
        }
        assert_eq!(result.unwrap()["message"]["content"], "héllo wörld");
        assert!(!asm.in_progress());
    }

    #[test]
    fn rejects_out_of_order_chunks() {
        let frames = chunks("c1", r#"{"type":"x","pad":"0123456789"}"#, 5);
        let mut asm = ChunkAssembler::default();
        asm.push(&frames[0]).unwrap();
        assert!(asm.push(&frames[2]).is_err());
        assert!(!asm.in_progress());
    }
}
