//! Batch Transaction Types

use alloc::vec::Vec;
use alloy::primitives::Bytes;
use kona_primitives::Frame;

/// BatchTransaction is a set of [Frame]s that can be [Into::into] [Bytes].
/// if the size exceeds the desired threshold.
#[derive(Debug, Clone)]
pub struct BatchTransaction {
    /// The frames in the batch.
    pub frames: Vec<Frame>,
    /// The size of the potential transaction.
    pub size: usize,
}

impl BatchTransaction {
    /// Returns the size of the transaction.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns if the transaction has reached the max frame count.
    pub fn is_full(&self, max_frames: u16) -> bool {
        self.frames.len() as u16 >= max_frames
    }

    /// Returns the concatenated encoding of the batch transaction frames as bytes.
    pub fn to_bytes(&self) -> Bytes {
        let mut buf: Vec<u8> = Vec::new();
        for frame in self.frames.iter() {
            buf.append(&mut frame.encode());
        }
        buf.into()
    }
}
