//! Contains the [RollupNode] trait.

use crate::types::SyncStatus;
use alloc::boxed::Box;
use async_trait::async_trait;
use eyre::Result;

/// A provider for the Rollup Node.
#[async_trait]
pub trait RollupNode {
    /// Fetches the sync status of the node.
    async fn sync_status(&self) -> Result<SyncStatus>;
}
