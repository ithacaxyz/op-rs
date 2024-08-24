//! Traits for interacting with serialization stages.

use crate::types::{BatchTransaction, L1BlockRef};
use alloc::boxed::Box;
use async_trait::async_trait;

/// Updates the stages that a new L1 block is canonical.
pub trait OriginReceiver {
    /// Sets a new origin.
    fn set_origin(&mut self, origin: L1BlockRef);
}

/// Updates the stages that a frame was published.
pub trait FramePublished {
    /// Notifies the stage that a frame was published.
    fn frame_published(&mut self, l1_block_num: u64);
}

/// Trait for producing the next transactions.
#[async_trait]
pub trait NextTransaction {
    /// Returns the next [BatchTransaction].
    async fn next_transaction(&mut self) -> Option<&BatchTransaction>;
}
