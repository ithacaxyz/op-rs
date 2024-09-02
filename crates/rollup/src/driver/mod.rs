//! Rollup Node Driver

use std::{fmt::Debug, sync::Arc};

use eyre::{bail, Result};
use kona_derive::{
    online::{AlloyChainProvider, AlloyL2ChainProvider, OnlineBlobProviderBuilder},
    traits::{BlobProvider, ChainProvider, L2ChainProvider},
};
use kona_primitives::BlockInfo;
use kona_providers::{
    blob_provider::DurableBlobProvider, InMemoryChainProvider, LayeredBlobProvider, Pipeline,
};
use reth_exex::ExExContext;
use reth_node_api::FullNodeComponents;
use superchain_registry::RollupConfig;
use tracing::{debug, info, warn};

use crate::{new_rollup_pipeline, HeraArgsExt, RollupPipeline};

mod context;
use context::{ChainNotification, DriverContext, StandaloneContext};

mod cursor;
use cursor::SyncCursor;

/// The Rollup Driver entrypoint.
#[derive(Debug)]
pub struct Driver<DC, CP, BP, L2CP> {
    /// The rollup configuration
    cfg: Arc<RollupConfig>,
    /// The context of the node
    ctx: DC,
    /// The L1 chain provider
    chain_provider: CP,
    /// The L1 blob provider
    blob_provider: BP,
    /// The L2 chain provider
    l2_chain_provider: L2CP,
    /// Cursor to keep track of the L2 tip
    cursor: SyncCursor,
}

impl<N> Driver<ExExContext<N>, InMemoryChainProvider, LayeredBlobProvider, AlloyL2ChainProvider>
where
    N: FullNodeComponents,
{
    /// Create a new Hera Execution Extension Driver
    pub fn exex(ctx: ExExContext<N>, args: HeraArgsExt, cfg: Arc<RollupConfig>) -> Self {
        let cp = InMemoryChainProvider::with_capacity(1024);
        let l2_cp = AlloyL2ChainProvider::new_http(args.l2_rpc_url, cfg.clone());
        let bp = LayeredBlobProvider::new(args.l1_beacon_client_url, args.l1_blob_archiver_url);

        Self {
            cfg,
            ctx,
            chain_provider: cp,
            blob_provider: bp,
            l2_chain_provider: l2_cp,
            cursor: SyncCursor::new(),
        }
    }
}

impl Driver<StandaloneContext, AlloyChainProvider, DurableBlobProvider, AlloyL2ChainProvider> {
    /// Create a new standalone Hera Driver
    pub fn standalone(ctx: StandaloneContext, args: HeraArgsExt, cfg: Arc<RollupConfig>) -> Self {
        let cp = AlloyChainProvider::new_http(args.l1_rpc_url);
        let l2_cp = AlloyL2ChainProvider::new_http(args.l2_rpc_url, cfg.clone());
        let bp = OnlineBlobProviderBuilder::new()
            .with_primary(args.l1_beacon_client_url.to_string())
            .with_fallback(args.l1_blob_archiver_url.map(|url| url.to_string()))
            .build();

        Self {
            cfg,
            ctx,
            chain_provider: cp,
            blob_provider: bp,
            l2_chain_provider: l2_cp,
            cursor: SyncCursor::new(),
        }
    }
}

impl<DC, CP, BP, L2CP> Driver<DC, CP, BP, L2CP>
where
    DC: DriverContext,
    CP: ChainProvider + Clone + Send + Sync + Debug + 'static,
    BP: BlobProvider + Clone + Send + Sync + Debug + 'static,
    L2CP: L2ChainProvider + Clone + Send + Sync + Debug + 'static,
{
    /// Wait for the L2 genesis L1 block (aka "origin block") to be available in the L1 chain.
    async fn wait_for_l2_genesis_l1_block(&mut self) -> Result<()> {
        loop {
            if let Some(notification) = self.ctx.recv_notification().await {
                if let Some(new_chain) = notification.new_chain() {
                    let tip = new_chain.tip();
                    // TODO: commit the chain to a local buffered provider
                    // self.chain_provider.commit_chain(new_chain);

                    if let Err(err) = self.ctx.send_processed_tip_event(tip) {
                        bail!("Critical: Failed to send processed tip event: {:?}", err);
                    }

                    if tip >= self.cfg.genesis.l1.number {
                        break Ok(());
                    } else {
                        debug!("Chain not yet synced to rollup genesis. L1 block number: {}", tip);
                    }
                }
            }
        }
    }

    /// Initialize the rollup pipeline from the driver's components.
    fn init_pipeline(&mut self) -> RollupPipeline<CP, BP, L2CP> {
        new_rollup_pipeline(
            self.cfg.clone(),
            self.chain_provider.clone(),
            self.blob_provider.clone(),
            self.l2_chain_provider.clone(),
            // TODO: use a dynamic "tip" block instead of genesis
            BlockInfo {
                hash: self.cfg.genesis.l2.hash,
                number: self.cfg.genesis.l2.number,
                ..Default::default()
            },
        )
    }

    /// Advance the pipeline to the next L2 block.
    async fn step(&mut self, pipeline: &mut RollupPipeline<CP, BP, L2CP>) -> Result<()> {
        let (l2_tip, l1_origin) = self.cursor.tip();
        let _ = pipeline.step(l2_tip).await;
        self.cursor.advance(l1_origin, l2_tip);

        unimplemented!()
    }

    /// Handle a chain notification from the driver context.
    async fn handle_notification(
        &mut self,
        notification: ChainNotification,
        pipeline: &mut RollupPipeline<CP, BP, L2CP>,
    ) -> Result<()> {
        if let Some(reverted_chain) = notification.reverted_chain() {
            // The reverted chain contains the list of blocks that were invalidated by the
            // reorg. we need to reset the cursor to the last canonical block, which corresponds
            // to the block before the reorg happened.
            let fork_block = reverted_chain.fork_block();

            // Find the last known L2 block that is still valid after the reorg,
            // and reset the cursor and pipeline to it.
            let (l2_safe_tip, l2_safe_tip_l1_origin) = self.cursor.reset(fork_block);

            warn!("Reverting derivation pipeline to L2 block: {}", l2_safe_tip.number);
            if let Err(e) = pipeline.reset(l2_safe_tip, l2_safe_tip_l1_origin).await {
                bail!("Failed to reset pipeline: {:?}", e);
            }
        }

        if let Some(new_chain) = notification.new_chain() {
            let tip = new_chain.tip();
            // self.chain_provider.commit_chain(new_chain);

            if let Err(err) = self.ctx.send_processed_tip_event(tip) {
                bail!("Critical: Failed to send processed tip event: {:?}", err);
            }
        }

        Ok(())
    }

    /// Starts the Hera derivation loop and tries to advance the driver to
    /// the L2 chain tip.
    ///
    /// # Errors
    ///
    /// This function should never error. If it does, it means the entire driver
    /// will shut down. If running as ExEx, the entire L1 node + all other running
    /// execution extensions will be shutdown as well.
    pub async fn start(mut self) -> Result<()> {
        // Step 1: Wait for the L2 origin block to be available
        self.wait_for_l2_genesis_l1_block().await?;
        info!("L1 chain synced to the rollup genesis block");

        // Step 2: Initialize the rollup pipeline
        let mut pipeline = self.init_pipeline();

        // Step 3: Start processing events
        loop {
            // TODO: handle pipeline step (stubbed)
            let _ = self.step(&mut pipeline).await;

            if let Some(notification) = self.ctx.recv_notification().await {
                self.handle_notification(notification, &mut pipeline).await?;
            }
        }
    }
}
