use alloy::primitives::BlockNumber;
use async_trait::async_trait;
use reth_exex::{ExExContext, ExExEvent};
use reth_node_api::FullNodeComponents;
use tokio::sync::mpsc::error::SendError;

use crate::driver::{ChainNotification, DriverContext};

#[async_trait]
impl<N: FullNodeComponents> DriverContext for ExExContext<N> {
    async fn recv_notification(&mut self) -> Option<ChainNotification> {
        let exex_notification = self.notifications.recv().await?;
        Some(ChainNotification::from(exex_notification))
    }

    fn send_processed_tip_event(&mut self, tip: BlockNumber) -> Result<(), SendError<BlockNumber>> {
        self.events.send(ExExEvent::FinishedHeight(tip)).map_err(|_| SendError(tip))
    }
}
