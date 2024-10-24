use alloy_eips::BlockNumHash;
use async_trait::async_trait;
use futures::StreamExt;
use kona_derive_local::InMemoryChainProvider;
use reth_exex::{ExExContext, ExExEvent};
use reth_node_api::FullNodeComponents;

use crate::driver::{ChainNotification, DriverContext};

/// Execution extension Hera context.
///
/// This context is used to bridge the gap between the execution extension
/// and the Hera pipeline. It receives notifications from the execution extension
/// and forwards them to the Hera pipeline. It also maintains a shared L1 cache of
/// the chain to make it available to the rollup pipeline.
pub struct ExExHeraContext<N: FullNodeComponents> {
    ctx: ExExContext<N>,
    l1_cache: InMemoryChainProvider,
}

impl<N: FullNodeComponents> ExExHeraContext<N> {
    /// Create a new execution extension Hera context with the given
    /// execution context and L1 cache.
    pub fn new(ctx: ExExContext<N>, l1_cache: InMemoryChainProvider) -> Self {
        Self { ctx, l1_cache }
    }
}

#[async_trait]
impl<N: FullNodeComponents> DriverContext for ExExHeraContext<N> {
    async fn recv_notification(&mut self) -> Option<ChainNotification> {
        let exex_notification = self.ctx.notifications.next().await?.ok()?;

        // Commit the new chain to the L1 cache to make it available to the pipeline
        if let Some(chain) = exex_notification.committed_chain() {
            self.l1_cache.commit(chain);
        }

        Some(ChainNotification::from(exex_notification))
    }

    fn send_processed_tip_event(&mut self, tip: BlockNumHash) {
        // The only way this unbounded sender can error is if the receiver is dropped.
        // For that to happen, the entire ExEx context would be dropped, so we can safely
        // ignore the error here.
        let _ = self.ctx.events.send(ExExEvent::FinishedHeight(tip));
    }
}
