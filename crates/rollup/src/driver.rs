//! Rollup Node Driver

use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Debug,
    sync::Arc,
};

use async_trait::async_trait;
use eyre::{bail, Result};
use kona_derive::{
    online::{AlloyChainProvider, AlloyL2ChainProvider, OnlineBlobProviderBuilder},
    traits::{BlobProvider, ChainProvider, L2ChainProvider},
};
use kona_primitives::{BlockInfo, L2BlockInfo};
use kona_providers::{
    blob_provider::DurableBlobProvider, InMemoryChainProvider, LayeredBlobProvider, Pipeline,
};
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use superchain_registry::RollupConfig;
use tokio::sync::mpsc::error::SendError;
use tracing::{debug, info, warn};

use crate::{new_rollup_pipeline, HeraArgsExt, RollupPipeline};

#[async_trait]
pub trait DriverContext {
    async fn recv_notification(&mut self) -> Option<ExExNotification>;

    fn send_event(&mut self, event: ExExEvent) -> Result<(), SendError<ExExEvent>>;
}

#[async_trait]
impl<N: FullNodeComponents> DriverContext for ExExContext<N> {
    async fn recv_notification(&mut self) -> Option<ExExNotification> {
        self.notifications.recv().await
    }

    fn send_event(&mut self, event: ExExEvent) -> Result<(), SendError<ExExEvent>> {
        self.events.send(event)
    }
}

#[derive(Debug)]
pub struct StandaloneContext;

#[async_trait]
impl DriverContext for StandaloneContext {
    async fn recv_notification(&mut self) -> Option<ExExNotification> {
        // TODO: we will need a background task to listen for new blocks
        // (either polling or via websocket), parse them into ExExNotifications
        // and handle reorgs for the driver to react to.
        todo!()
    }

    fn send_event(&mut self, _event: ExExEvent) -> Result<(), SendError<ExExEvent>> {
        // TODO: When we have a better background notifier abstraction, sending
        // FinishedHeight events will be useful to make the pipeline advance
        // safely through reorgs (as it is currently done for ExExContext).
        Ok(())
    }
}

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
    /// Cursor to keep track of the current L1 and L2 tip blocks
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
            cursor: SyncCursor::with_capacity(128),
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
            cursor: SyncCursor::with_capacity(128),
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
                if let Some(committed_chain) = notification.committed_chain() {
                    let tip = committed_chain.tip().block.header().number;
                    // TODO: commit the chain to a local buffered provider
                    // self.chain_provider.commit_chain(committed_chain);

                    if let Err(err) = self.ctx.send_event(ExExEvent::FinishedHeight(tip)) {
                        bail!("Critical: Failed to send ExEx event: {:?}", err);
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

    async fn handle_notification(
        &mut self,
        notif: ExExNotification,
        pipeline: &mut RollupPipeline<CP, BP, L2CP>,
    ) -> Result<()> {
        if let Some(reverted_chain) = notif.reverted_chain() {
            let last_canonical_block = reverted_chain.first().number - 1;
            let first_reorged_block = info_from_header(&reverted_chain.first().block);

            let l2_safe_tip = self.cursor.reset(last_canonical_block);
            if let Err(e) = pipeline.reset(l2_safe_tip, first_reorged_block).await {
                bail!("Failed to reset pipeline: {:?}", e);
            }
        }

        if let Some(committed_chain) = notif.committed_chain() {
            let tip_number = committed_chain.tip().number;
            // self.chain_provider.commit_chain(committed_chain);

            if let Err(err) = self.ctx.send_event(ExExEvent::FinishedHeight(tip_number)) {
                bail!("Critical: Failed to send ExEx event: {:?}", err);
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
        info!("Chain synced to rollup genesis");

        // Step 2: Initialize the rollup pipeline
        let mut pipeline = self.init_pipeline();

        // Step 3: Start processing events
        loop {
            if let Some(notification) = self.ctx.recv_notification().await {
                self.handle_notification(notification, &mut pipeline).await?;
            }
        }
    }
}

#[allow(unused)]
#[derive(Debug, Default)]
pub struct SyncCursor {
    /// The current L1 origin tip block number
    l1_tip: u64,
    /// The current L2 tip block number
    l2_tip: u64,
    /// The block cache capacity before evicting old entries
    /// (to avoid unbounded memory growth)
    capacity: usize,
    /// The L1 origin block info for which we have an L2 block in the cache.
    /// Used to keep track of the order of insertion and evict the oldest entry.
    l1_origin_block_info: VecDeque<BlockInfo>,
    /// Map of the L1 origin block number to its corresponding L2 tip block info
    l1_origin_to_l2_blocks: BTreeMap<u64, L2BlockInfo>,
}

#[allow(unused)]
impl SyncCursor {
    /// Create a new cursor with the given cache capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            l1_origin_block_info: VecDeque::with_capacity(capacity),
            l1_origin_to_l2_blocks: BTreeMap::new(),
            ..Default::default()
        }
    }

    pub fn l1_tip(&self) -> u64 {
        self.l1_tip
    }

    pub fn l2_tip(&self) -> u64 {
        self.l2_tip
    }

    /// Advance the cursor to the provided L2 block, given the corresponding L1 origin block.
    /// This is a no-op if the L2 origin block does not match the provided L1 block.
    pub fn advance_tip(&mut self, l1_block: BlockInfo, l2_block: L2BlockInfo) {
        if l2_block.l1_origin.hash != l1_block.hash {
            warn!(
                "Unexpected L1 origin block: {} (expected {})",
                l1_block.hash, l2_block.l1_origin.hash
            );
            return;
        }

        self.l1_tip = l1_block.number;
        self.l2_tip = l2_block.block_info.number;
        self.insert(l1_block, l2_block);
    }

    /// When the L1 undergoes a reorg, we need to reset the cursor
    /// to the last canonical known L1 block for which we have a
    /// corresponding entry in the cache.
    ///
    /// Returns the L2 block info for the new safe tip.
    pub fn reset(&mut self, last_canonical_l1_block_number: u64) -> BlockInfo {
        match self.l1_origin_to_l2_blocks.get(&last_canonical_l1_block_number) {
            Some(l2_safe_tip) => {
                self.l1_tip = last_canonical_l1_block_number;
                self.l2_tip = l2_safe_tip.block_info.number;

                l2_safe_tip.block_info
            }
            None => {
                // If the last canonical L1 block is not in the cache,
                // we reset the cursor to the last known L1 block for which
                // we have a corresponding L2 block.
                let (last_l1_known_tip, l2_known_tip) = self
                    .l1_origin_to_l2_blocks
                    .range(..=last_canonical_l1_block_number)
                    .next_back()
                    .expect("walked back to genesis without finding anchor origin block");

                self.l1_tip = *last_l1_known_tip;
                self.l2_tip = l2_known_tip.block_info.number;

                l2_known_tip.block_info
            }
        }
    }

    /// Insert a new L1 origin block info and its corresponding
    /// tip L2 block info into the cache.
    ///
    /// If the cache is full, the oldest entry is evicted.
    fn insert(&mut self, l1_block: BlockInfo, l2_block: L2BlockInfo) {
        if self.l1_origin_to_l2_blocks.len() >= self.capacity {
            let key = self.l1_origin_block_info.pop_front().unwrap();
            self.l1_origin_to_l2_blocks.remove(&key.number);
        }

        self.l1_origin_block_info.push_back(l1_block);
        self.l1_origin_to_l2_blocks.insert(l1_block.number, l2_block);
    }
}

/// Helper to extract block info from a sealed block
fn info_from_header(block: &reth::primitives::SealedBlock) -> BlockInfo {
    BlockInfo {
        hash: block.hash(),
        number: block.number,
        timestamp: block.timestamp,
        parent_hash: block.parent_hash,
    }
}
