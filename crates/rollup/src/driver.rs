//! Rollup Node Driver

use std::sync::Arc;

use async_trait::async_trait;
use eyre::{bail, Result};
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use superchain_registry::RollupConfig;
use tokio::sync::mpsc::error::SendError;
use tracing::{debug, info};

use crate::cli::HeraArgsExt;

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

/// The Rollup Driver entrypoint.
#[derive(Debug)]
pub struct Driver<DC: DriverContext> {
    /// The rollup configuration
    cfg: Arc<RollupConfig>,
    /// The context of the node
    ctx: DC,
}

#[allow(unused)]
impl<DC: DriverContext> Driver<DC> {
    /// Creates a new instance of the Hera Execution Extension.
    pub async fn new(ctx: DC, args: HeraArgsExt, cfg: Arc<RollupConfig>) -> Self {
        Self { ctx, cfg }
    }

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

    /// Starts the Hera Execution Extension loop.
    pub async fn start(mut self) -> Result<()> {
        // Step 1: Wait for the L2 origin block to be available
        self.wait_for_l2_genesis_l1_block().await?;
        info!("Chain synced to rollup genesis");

        todo!("init pipeline and start processing events");
    }
}
