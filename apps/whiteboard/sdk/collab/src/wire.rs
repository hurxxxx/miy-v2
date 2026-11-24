use anyhow::{Result, ensure};
use axum::extract::ws::{Message, WebSocket};

pub const LIMIT: usize = 16 * 1024 * 1024;
const FRAME: usize = 65536;
const PAYLOAD: usize = FRAME - 8;

pub async fn send(socket: &mut WebSocket, data: &[u8]) -> Result<()> {
    ensure!(
        !data.is_empty() && data.len() <= LIMIT,
        "invalid message length"
    );
    for (index, chunk) in data.chunks(PAYLOAD).enumerate() {
        let mut frame = Vec::with_capacity(8 + chunk.len());
        frame.extend_from_slice(&(data.len() as u32).to_be_bytes());
        frame.extend_from_slice(&((index * PAYLOAD) as u32).to_be_bytes());
        frame.extend_from_slice(chunk);
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            socket.send(Message::Binary(frame.into())),
        )
        .await??;
    }
    Ok(())
}

#[derive(Default)]
pub struct Reader {
    total: usize,
    bytes: Vec<u8>,
}

impl Reader {
    pub fn push(&mut self, frame: &[u8]) -> Result<Option<Vec<u8>>> {
        ensure!((8..=FRAME).contains(&frame.len()), "invalid frame length");
        let total = u32::from_be_bytes(frame[..4].try_into()?) as usize;
        let offset = u32::from_be_bytes(frame[4..8].try_into()?) as usize;
        ensure!(
            total > 0 && total <= LIMIT && offset == self.bytes.len() && offset < total,
            "invalid offset"
        );
        ensure!(offset == 0 || total == self.total, "interleaved frames");
        ensure!(
            frame.len() - 8 == (total - offset).min(PAYLOAD),
            "invalid chunk size"
        );
        self.total = total;
        self.bytes.extend_from_slice(&frame[8..]);
        Ok((self.bytes.len() == total).then(|| std::mem::take(&mut self.bytes)))
    }
}

pub fn packet(tag: u8, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 1);
    out.push(tag);
    out.extend_from_slice(data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_ordered_messages_only() {
        let mut reader = Reader::default();
        let frame = |n: u32, offset: u32, size: usize| {
            let mut f = n.to_be_bytes().to_vec();
            f.extend(offset.to_be_bytes());
            f.resize(8 + size, 42);
            f
        };
        let total = PAYLOAD as u32 + 17;
        assert!(reader.push(&frame(total, 0, PAYLOAD)).unwrap().is_none());
        assert!(reader.push(&frame(total, 0, PAYLOAD)).is_err());
        assert_eq!(
            reader
                .push(&frame(total, PAYLOAD as u32, 17))
                .unwrap()
                .unwrap()
                .len(),
            total as usize
        );
        assert!(reader.push(&frame(LIMIT as u32 + 1, 0, 0)).is_err());
    }
}
