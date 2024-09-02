//! Rollup Node Driver

use std::{fmt::Debug, sync::Arc};

use eyre::{bail, Result};
use kona_derive::{
    errors::StageError,
    online::{AlloyChainProvider, AlloyL2ChainProvider, OnlineBlobProviderBuilder},
    traits::{BlobProvider, ChainProvider, L2ChainProvider},
};
use kona_primitives::{BlockInfo, L2BlockInfo};
use kona_providers::{
    blob_provider::DurableBlobProvider, InMemoryChainProvider, LayeredBlobProvider, Pipeline,
    StepResult,
};
use reth::rpc::types::engine::JwtSecret;
use reth_exex::ExExContext;
use reth_node_api::FullNodeComponents;
use superchain_registry::RollupConfig;
use tracing::{debug, error, info, trace, warn};

use crate::{
    cli::ValidationMode,
    new_rollup_pipeline,
    validator::{EngineApiValidator, TrustedValidator},
    AttributesValidator, HeraArgsExt, RollupPipeline,
};

mod context;
pub use context::StandaloneContext;
use context::{ChainNotification, DriverContext};

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
    /// The validator to verify newly derived L2 attributes
    validator: Box<dyn AttributesValidator>,
}

impl<N> Driver<ExExContext<N>, InMemoryChainProvider, LayeredBlobProvider, AlloyL2ChainProvider>
where
    N: FullNodeComponents,
{
    /// Create a new Hera Execution Extension Driver
    pub fn exex(ctx: ExExContext<N>, args: HeraArgsExt, cfg: Arc<RollupConfig>) -> Self {
        let cp = InMemoryChainProvider::with_capacity(args.in_mem_chain_provider_capacity);
        let l2_cp = AlloyL2ChainProvider::new_http(args.l2_rpc_url.clone(), cfg.clone());
        let bp = LayeredBlobProvider::new(
            args.l1_beacon_client_url.clone(),
            args.l1_blob_archiver_url.clone(),
        );

        Self::with_components(ctx, args, cfg, cp, bp, l2_cp)
    }
}

impl Driver<StandaloneContext, AlloyChainProvider, DurableBlobProvider, AlloyL2ChainProvider> {
    /// Create a new Standalone Hera Driver
    pub fn standalone(ctx: StandaloneContext, args: HeraArgsExt, cfg: Arc<RollupConfig>) -> Self {
        let cp = AlloyChainProvider::new_http(args.l1_rpc_url.clone());
        let l2_cp = AlloyL2ChainProvider::new_http(args.l2_rpc_url.clone(), cfg.clone());
        let bp = OnlineBlobProviderBuilder::new()
            .with_primary(args.l1_beacon_client_url.to_string())
            .with_fallback(args.l1_blob_archiver_url.clone().map(|url| url.to_string()))
            .build();

        Self::with_components(ctx, args, cfg, cp, bp, l2_cp)
    }
}

impl<DC, CP, BP, L2CP> Driver<DC, CP, BP, L2CP>
where
    DC: DriverContext,
    CP: ChainProvider + Clone + Send + Sync + Debug + 'static,
    BP: BlobProvider + Clone + Send + Sync + Debug + 'static,
    L2CP: L2ChainProvider + Clone + Send + Sync + Debug + 'static,
{
    /// Create a new Hera Driver with the provided components.
    fn with_components(
        ctx: DC,
        args: HeraArgsExt,
        cfg: Arc<RollupConfig>,
        chain_provider: CP,
        blob_provider: BP,
        l2_chain_provider: L2CP,
    ) -> Self {
        let cursor = SyncCursor::new(cfg.channel_timeout);
        let validator: Box<dyn AttributesValidator> = match args.validation_mode {
            ValidationMode::Trusted => {
                Box::new(TrustedValidator::new_http(args.l2_rpc_url, cfg.canyon_time.unwrap_or(0)))
            }
            ValidationMode::EngineApi => Box::new(EngineApiValidator::new_http(
                args.l2_engine_api_url.expect("Missing L2 engine API URL"),
                match args.l2_engine_jwt_secret.as_ref() {
                    Some(fpath) => JwtSecret::from_file(fpath).expect("Invalid L2 JWT secret file"),
                    None => panic!("Missing L2 engine JWT secret"),
                },
            )),
        };

        Self { cfg, ctx, chain_provider, blob_provider, l2_chain_provider, cursor, validator }
    }

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
    ///
    /// Returns `true` if the pipeline can move forward again, `false` otherwise.
    async fn step(&mut self, pipeline: &mut RollupPipeline<CP, BP, L2CP>) -> bool {
        let l2_tip = self.cursor.tip();

        match pipeline.step(l2_tip).await {
            StepResult::PreparedAttributes => trace!("Perpared new attributes"),
            StepResult::AdvancedOrigin => trace!("Advanced origin"),
            StepResult::OriginAdvanceErr(err) => warn!("Could not advance origin: {:?}", err),
            StepResult::StepFailed(err) => match err {
                StageError::NotEnoughData => debug!("Not enough data to advance pipeline"),
                _ => error!("Error stepping derivation pipeline: {:?}", err),
            },
        }

        let derived_attributes = if let Some(attributes) = pipeline.peek() {
            match self.validator.validate(attributes).await {
                Ok(true) => {
                    trace!("Validated payload attributes");
                    pipeline.next().expect("Peeked attributes must be available")
                }
                Ok(false) => {
                    error!("Failed payload attributes validation");
                    // TODO: allow users to specify how they want to treat invalid payloads.
                    // In the default scenario we just log an error and continue.
                    return false;
                }
                Err(err) => {
                    error!("Error while validating payload attributes: {:?}", err);
                    return false;
                }
            }
        } else {
            debug!("No attributes available to validate");
            return false;
        };

        let derived = derived_attributes.parent.block_info.number + 1;
        let (new_l1_origin, new_l2_tip) = match self.fetch_new_tip(derived).await {
            Ok(tip_info) => tip_info,
            Err(err) => {
                // TODO: add a retry mechanism?
                error!("Failed to fetch new tip: {:?}", err);
                return false;
            }
        };

        // Perform a sanity check on the new tip
        if new_l2_tip.block_info.number != derived {
            error!("Expected L2 block number {} but got {}", derived, new_l2_tip.block_info.number);
            return false;
        }

        // Advance the cursor to the new L2 block
        self.cursor.advance(new_l1_origin, new_l2_tip);
        info!("Advanced derivation pipeline to L2 block: {}", derived);
        true
    }

    /// Fetch the new L2 tip and L1 origin block info for the given L2 block number.
    async fn fetch_new_tip(&mut self, l2_tip: u64) -> Result<(BlockInfo, L2BlockInfo)> {
        let l2_block = self
            .l2_chain_provider
            .l2_block_info_by_number(l2_tip)
            .await
            .map_err(|e| eyre::eyre!(e))?;

        let l1_origin = self
            .chain_provider
            .block_info_by_number(l2_block.l1_origin.number)
            .await
            .map_err(|e| eyre::eyre!(e))?;

        Ok((l1_origin, l2_block))
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

        // Step 3: Start the processing loop
        loop {
            // Try to advance the pipeline until there's no more data to process
            if self.step(&mut pipeline).await {
                continue;
            }

            // Handle any incoming notifications from the context
            if let Some(notification) = self.ctx.recv_notification().await {
                self.handle_notification(notification, &mut pipeline).await?;
            }
        }
    }
}
