//! Rollup Node Driver

use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use eyre::{bail, Result};
use kona_derive::{
    online::{AlloyChainProvider, AlloyL2ChainProvider, OnlineBlobProviderBuilder},
    traits::{BlobProvider, ChainProvider, L2ChainProvider},
};
use kona_primitives::BlockInfo;
use kona_providers::{
    blob_provider::DurableBlobProvider, InMemoryChainProvider, LayeredBlobProvider,
};
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use superchain_registry::RollupConfig;
use tokio::sync::mpsc::error::SendError;
use tracing::{debug, info};

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
}

impl<N> Driver<ExExContext<N>, InMemoryChainProvider, LayeredBlobProvider, AlloyL2ChainProvider>
where
    N: FullNodeComponents,
{
    /// Create a new Hera Execution Extension Driver
    pub fn exex(ctx: ExExContext<N>, args: HeraArgsExt, cfg: Arc<RollupConfig>) -> Self {
        let cp = InMemoryChainProvider::with_capacity(1024);
        let bp = LayeredBlobProvider::new(args.l1_beacon_client_url, args.l1_blob_archiver_url);
        let l2_cp = AlloyL2ChainProvider::new_http(args.l2_rpc_url, cfg.clone());

        Self { cfg, ctx, chain_provider: cp, blob_provider: bp, l2_chain_provider: l2_cp }
    }
}

impl Driver<StandaloneContext, AlloyChainProvider, DurableBlobProvider, AlloyL2ChainProvider> {
    /// Create a new standalone Hera Driver
    pub fn std(ctx: StandaloneContext, args: HeraArgsExt, cfg: Arc<RollupConfig>) -> Self {
        let cp = AlloyChainProvider::new_http(args.l1_rpc_url);
        let l2_cp = AlloyL2ChainProvider::new_http(args.l2_rpc_url, cfg.clone());
        let bp = OnlineBlobProviderBuilder::new()
            .with_primary(args.l1_beacon_client_url.to_string())
            .with_fallback(args.l1_blob_archiver_url.map(|url| url.to_string()))
            .build();

        Self { cfg, ctx, chain_provider: cp, blob_provider: bp, l2_chain_provider: l2_cp }
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
        if let Some(_reverted_chain) = notif.reverted_chain() {
            // handle the reverted chain...
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
