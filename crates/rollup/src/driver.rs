//! Rollup Node Driver

use hashbrown::HashMap;
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

    /// Advance the pipeline to the next L2 block.
    async fn step(&mut self, pipeline: &mut RollupPipeline<CP, BP, L2CP>) -> Result<()> {
        let (l2_tip, l1_origin) = self.cursor.tip();
        let _ = pipeline.step(l2_tip).await;
        self.cursor.advance(l1_origin, l2_tip);

        unimplemented!()
    }

    async fn handle_notification(
        &mut self,
        notif: ExExNotification,
        pipeline: &mut RollupPipeline<CP, BP, L2CP>,
    ) -> Result<()> {
        if let Some(reverted_chain) = notif.reverted_chain() {
            // The reverted chain contains the list of blocks that were invalidated by the
            // reorg. we need to reset the cursor to the last canonical block, which corresponds
            // to the block before the reorg happened.
            let last_canonical_block = reverted_chain.first().number - 1;

            // Find the last known L2 block that is still valid after the reorg,
            // and reset the cursor and pipeline to it.
            let (l2_safe_tip, l2_safe_tip_l1_origin) = self.cursor.reset(last_canonical_block);

            warn!("Reverting derivation pipeline to L2 block: {}", l2_safe_tip.number);
            if let Err(e) = pipeline.reset(l2_safe_tip, l2_safe_tip_l1_origin).await {
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

/// A cursor that keeps track of the L2 tip block for a given L1 origin block.
///
/// The cursor is used to advance the pipeline to the next L2 block, and to reset
/// the pipeline to a previous L2 block when a reorg happens.
#[derive(Debug)]
pub struct SyncCursor {
    /// The block cache capacity before evicting old entries
    /// (to avoid unbounded memory growth)
    capacity: usize,
    /// The L1 origin block numbers for which we have an L2 block in the cache.
    /// Used to keep track of the order of insertion and evict the oldest entry.
    l1_origin_key_order: VecDeque<u64>,
    /// The L1 origin block info for which we have an L2 block in the cache.
    l1_origin_block_info: HashMap<u64, BlockInfo>,
    /// Map of the L1 origin block number to its corresponding tip L2 block
    l1_origin_to_l2_blocks: BTreeMap<u64, L2BlockInfo>,
}

#[allow(unused)]
impl SyncCursor {
    /// Create a new cursor with the default cache capacity.
    pub fn new() -> Self {
        // NOTE: this value must be greater than the `CHANNEL_TIMEOUT` to allow
        // for derivation to proceed through a deep reorg. This value is set
        // to 300 blocks before the Granite hardfork and 50 blocks after it.
        // Ref: <https://specs.optimism.io/protocol/derivation.html#timeouts>
        Self::with_capacity(350)
    }

    /// Create a new cursor with the given cache capacity.
    fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            l1_origin_key_order: VecDeque::with_capacity(capacity),
            l1_origin_block_info: HashMap::with_capacity(capacity),
            l1_origin_to_l2_blocks: BTreeMap::new(),
        }
    }

    /// Get the current L2 tip and the corresponding L1 origin block info.
    pub fn tip(&self) -> (L2BlockInfo, BlockInfo) {
        if let Some((origin_number, l2_tip)) = self.l1_origin_to_l2_blocks.last_key_value() {
            let origin_block = self.l1_origin_block_info[origin_number];
            (*l2_tip, origin_block)
        } else {
            unreachable!("cursor must be initialized with one block before advancing")
        }
    }

    /// Advance the cursor to the provided L2 block, given the corresponding L1 origin block.
    ///
    /// If the cache is full, the oldest entry is evicted.
    pub fn advance(&mut self, l1_origin_block: BlockInfo, l2_tip_block: L2BlockInfo) {
        if self.l1_origin_to_l2_blocks.len() >= self.capacity {
            let key = self.l1_origin_key_order.pop_front().unwrap();
            self.l1_origin_to_l2_blocks.remove(&key);
        }

        self.l1_origin_key_order.push_back(l1_origin_block.number);
        self.l1_origin_block_info.insert(l1_origin_block.number, l1_origin_block);
        self.l1_origin_to_l2_blocks.insert(l1_origin_block.number, l2_tip_block);
    }

    /// When the L1 undergoes a reorg, we need to reset the cursor to the last canonical
    /// known L1 block for which we have a corresponding entry in the cache.
    ///
    /// Returns the (L2 block info, L1 origin block info) tuple for the new cursor state.
    pub fn reset(&mut self, last_canonical_l1_block_number: u64) -> (BlockInfo, BlockInfo) {
        match self.l1_origin_to_l2_blocks.get(&last_canonical_l1_block_number) {
            Some(l2_safe_tip) => {
                // The last canonical L1 block is in the cache, we can use it to reset the cursor.
                // INVARIANT: the L1 origin info must be present in the cache
                // since we always insert them in tandem.
                (l2_safe_tip.block_info, self.l1_origin_block_info[&last_canonical_l1_block_number])
            }
            None => {
                // If the last canonical L1 block is not in the cache, we reset the cursor
                // to the last known L1 block for which we have a corresponding L2 block.
                let (last_l1_known_tip, l2_known_tip) = self
                    .l1_origin_to_l2_blocks
                    .range(..=last_canonical_l1_block_number)
                    .next_back()
                    .expect("walked back to genesis without finding anchor origin block");

                // INVARIANT: the L1 origin info must be present in the cache
                // since we always insert them in tandem.
                (l2_known_tip.block_info, self.l1_origin_block_info[last_l1_known_tip])
            }
        }
    }
}
