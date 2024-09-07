use futures::StreamExt;
use hashbrown::HashMap;
use std::collections::BTreeMap;

use alloy::{
    network::Ethereum,
    primitives::{BlockNumber, B256},
    providers::{IpcConnect, Provider, ProviderBuilder, ReqwestProvider, WsConnect},
    rpc::types::Block,
    transports::{TransportErrorKind, TransportResult},
};
use async_trait::async_trait;
use kona_primitives::TxEnvelope;
use reth::rpc::types::{BlockTransactions, BlockTransactionsKind};
use tokio::{
    sync::mpsc::{self, error::SendError},
    task::JoinHandle,
};
use tracing::{debug, error};
use url::Url;

use super::{Blocks, ChainNotification, DriverContext};

/// The number of blocks to keep in the reorg cache.
/// Equivalent to 2 epochs at 32 slots/epoch on Ethereum Mainnet.
const FINALIZATION_TIMEOUT: u64 = 64;

#[allow(unused)]
#[derive(Debug)]
pub struct StandaloneContext {
    /// The current tip of the L1 chain listener, used to detect reorgs
    l1_tip: BlockNumber,
    /// Channel that receives new blocks from the L1 node
    new_block_rx: mpsc::Receiver<Block<TxEnvelope>>,
    /// The highest block that was successfully processed by the driver.
    /// We can safely prune all cached blocks below this tip once they
    /// become finalized on L1.
    processed_tip: BlockNumber,
    /// Cache of blocks that might be reorged out. In normal conditions,
    /// this cache will not grow beyond [`FINALIZATION_TIMEOUT`] keys.
    reorg_cache: BTreeMap<BlockNumber, HashMap<B256, Block<TxEnvelope>>>,
    /// Handle to the background task that fetches and processes new blocks.
    _handle: JoinHandle<()>,
}

impl StandaloneContext {
    /// Create a new standalone context that polls for new chains.
    pub async fn new(l1_rpc_url: Url) -> TransportResult<Self> {
        if l1_rpc_url.scheme().contains("http") {
            debug!("Polling for new blocks via HTTP");
            Self::with_http_poller(l1_rpc_url).await
        } else if l1_rpc_url.scheme().contains("ws") {
            debug!("Subscribing to new blocks via websocket/ipc");
            Self::with_ws_subscriber(l1_rpc_url).await
        } else if l1_rpc_url.scheme().contains("file") {
            debug!("Subscribing to new blocks via IPC");
            Self::with_ipc_subscriber(l1_rpc_url).await
        } else {
            Err(TransportErrorKind::custom_str("Unsupported URL scheme"))
        }
    }

    /// Create a new standalone context that polls for new blocks via HTTP.
    async fn with_http_poller(l1_rpc_url: Url) -> TransportResult<Self> {
        let client = ReqwestProvider::<Ethereum>::new_http(l1_rpc_url);
        let (new_block_tx, new_block_rx) = mpsc::channel(128);

        let mut new_block_hashes = client.watch_blocks().await?.into_stream();
        let _handle = tokio::spawn(async move {
            while let Some(hashes) = new_block_hashes.next().await {
                for hash in hashes {
                    match client.get_block_by_hash(hash, BlockTransactionsKind::Full).await {
                        Ok(Some(block)) => {
                            let block_with_txs = parse_reth_rpc_block(block);
                            if let Err(e) = new_block_tx.try_send(block_with_txs) {
                                error!("Failed to send new block to channel: {:?}", e);
                            }
                        }
                        Ok(None) => {
                            // Question: does `eth_getBlockByHash` return a block if it was
                            // already reorged out? No-op for now.
                        }
                        Err(e) => {
                            error!("Failed to get block by hash: {:?}", e);
                        }
                    }
                }
            }
        });

        Ok(Self::with_defaults(new_block_rx, _handle))
    }

    /// Create a new standalone context that subscribes to new blocks via websocket.
    async fn with_ws_subscriber(l1_rpc_url: Url) -> TransportResult<Self> {
        let ws = WsConnect::new(l1_rpc_url);
        let client = ProviderBuilder::new().on_ws(ws).await?;
        let (new_block_tx, new_block_rx) = mpsc::channel(128);

        let mut block_sub = client.subscribe_blocks().await?.into_stream();
        let _handle = tokio::spawn(async move {
            while let Some(block) = block_sub.next().await {
                let block_with_txs_decoded = parse_reth_rpc_block(block);
                if let Err(e) = new_block_tx.try_send(block_with_txs_decoded) {
                    error!("Failed to send new block to channel: {:?}", e);
                }
            }
        });

        Ok(Self::with_defaults(new_block_rx, _handle))
    }

    /// Create a new standalone context that subscribes to new blocks via IPC.
    async fn with_ipc_subscriber(l1_rpc_url: Url) -> TransportResult<Self> {
        let ipc = IpcConnect::new(l1_rpc_url.to_file_path().expect("must be a file path"));
        let client = ProviderBuilder::new().on_ipc(ipc).await?;
        let (new_block_tx, new_block_rx) = mpsc::channel(128);

        let mut block_sub = client.subscribe_blocks().await?.into_stream();
        let _handle = tokio::spawn(async move {
            while let Some(block) = block_sub.next().await {
                let block_with_txs_decoded = parse_reth_rpc_block(block);
                if let Err(e) = new_block_tx.try_send(block_with_txs_decoded) {
                    error!("Failed to send new block to channel: {:?}", e);
                }
            }
        });

        Ok(Self::with_defaults(new_block_rx, _handle))
    }

    /// Create a new standalone context with the given new block receiver and handle.
    fn with_defaults(
        new_block_rx: mpsc::Receiver<Block<TxEnvelope>>,
        _handle: JoinHandle<()>,
    ) -> Self {
        Self { new_block_rx, _handle, l1_tip: 0, processed_tip: 0, reorg_cache: BTreeMap::new() }
    }
}

#[async_trait]
impl DriverContext for StandaloneContext {
    async fn recv_notification(&mut self) -> Option<ChainNotification> {
        // TODO: is it ok to skip fetching full txs and receipts here assuming the node will
        // have a fallback online RPC for that downstream? The driver and provider should be
        // generic but currently are very coupled to the node mode (standalone vs exex).

        let block = self.new_block_rx.recv().await?;
        let block_num = block.header.number;

        let entry = self.reorg_cache.entry(block_num).or_default();
        entry.insert(block.header.hash, block.clone());

        if block_num <= self.l1_tip {
            todo!("handle reorgs");
        } else {
            self.l1_tip = block_num;

            // upon a new tip, prune the reorg cache for all blocks that have been finalized,
            // as they are no longer candidates for reorgs.
            self.reorg_cache.retain(|num, _| *num > block_num - FINALIZATION_TIMEOUT);
        }

        Some(ChainNotification::New { new_blocks: Blocks::from(block) })
    }

    fn send_processed_tip_event(&mut self, tip: BlockNumber) -> Result<(), SendError<BlockNumber>> {
        self.processed_tip = tip;
        Ok(())
    }
}

// from reth::rpc::types::Block to alloy::rpc::types::Block<TxEnvelope>
fn parse_reth_rpc_block(block: Block) -> Block<TxEnvelope> {
    let txs = block
        .transactions
        .as_transactions()
        .unwrap_or_default()
        .iter()
        .cloned()
        .flat_map(|tx| TxEnvelope::try_from(tx).ok())
        .collect::<Vec<_>>();

    Block {
        header: block.header,
        uncles: block.uncles,
        transactions: BlockTransactions::Full(txs),
        size: block.size,
        withdrawals: block.withdrawals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::rpc::types::Header;

    #[tokio::test]
    async fn test_http_poller() -> eyre::Result<()> {
        let url = Url::parse("http://localhost:8545")?;

        // skip test if we can't connect to the node
        if reqwest::get(url.clone()).await.is_err() {
            return Ok(());
        }

        let mut ctx = StandaloneContext::new(url).await?;

        let notif = ctx.recv_notification().await.unwrap();

        assert!(notif.new_chain().is_some());
        assert!(notif.reverted_chain().is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_ws_subscriber() -> eyre::Result<()> {
        let url = Url::parse("ws://localhost:8546")?;

        // skip test if we can't connect to the node
        if reqwest::get(url.clone()).await.is_err() {
            return Ok(());
        }

        let mut ctx = StandaloneContext::new(url).await?;

        let notif = ctx.recv_notification().await.unwrap();

        assert!(notif.new_chain().is_some());
        assert!(notif.reverted_chain().is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_reorg_cache_pruning() {
        let (_, rx) = mpsc::channel(128);
        let handle = tokio::spawn(async {});
        let mut ctx = StandaloneContext::with_defaults(rx, handle);

        // Simulate receiving 100 blocks
        for i in 1..=100 {
            let block = create_mock_block(i);
            ctx.reorg_cache.entry(i).or_default().insert(block.header.hash, block);
        }

        // Simulate receiving a new block that should trigger pruning
        let new_block = create_mock_block(101);
        ctx.l1_tip = 101;
        ctx.reorg_cache.entry(101).or_default().insert(new_block.header.hash, new_block.clone());

        // Manually call the pruning logic
        ctx.reorg_cache.retain(|num, _| *num > 101 - FINALIZATION_TIMEOUT);

        // Check that only the last FINALIZATION_BLOCKS are kept
        assert_eq!(ctx.reorg_cache.len(), FINALIZATION_TIMEOUT as usize);
        assert!(ctx.reorg_cache.contains_key(&(101 - FINALIZATION_TIMEOUT + 1)));
        assert!(!ctx.reorg_cache.contains_key(&(101 - FINALIZATION_TIMEOUT)));
    }

    #[tokio::test]
    async fn test_send_processed_tip_event() {
        let (_, rx) = mpsc::channel(128);
        let handle = tokio::spawn(async {});
        let mut ctx = StandaloneContext::with_defaults(rx, handle);

        // Send a processed tip event
        let result = ctx.send_processed_tip_event(100);
        assert!(result.is_ok());
    }

    // Helper function to create a mock Block<TxEnvelope>
    fn create_mock_block(number: u64) -> Block<TxEnvelope> {
        Block {
            header: Header { number, hash: B256::random(), ..Default::default() },
            transactions: BlockTransactions::Full(vec![]),
            uncles: vec![],
            size: None,
            withdrawals: None,
        }
    }
}
