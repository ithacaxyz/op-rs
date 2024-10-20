use hashbrown::HashMap;
use std::{collections::BTreeMap, time::Duration};

use alloy_eips::{eip1898::BlockNumHash, BlockId};
use alloy_network::Ethereum;
use alloy_primitives::{BlockNumber, B256};
use alloy_provider::{IpcConnect, Provider, ProviderBuilder, ReqwestProvider, WsConnect};
use alloy_rpc_types_eth::Block;
use alloy_transport::{TransportErrorKind, TransportResult};

use async_trait::async_trait;
use futures::StreamExt;
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{debug, error, warn};
use url::Url;

use super::{Blocks, ChainNotification, DriverContext};

/// The number of blocks to keep in the reorg cache.
/// Equivalent to 2 epochs at 32 slots/epoch on Ethereum Mainnet.
const FINALIZATION_TIMEOUT: u64 = 64;

/// A standalone context that polls for new blocks from an L1 node, depending
/// on the URL scheme. Supported schemes are `http`, `ws`, and `file`.
///
/// If the URL scheme is `http`, the context will poll for new blocks using the
/// `eth_getFilterChanges` if available, falling back to `eth_getBlockByNumber` if not.
#[derive(Debug)]
pub struct StandaloneHeraContext {
    /// The current tip of the L1 chain listener, used to detect reorgs
    l1_tip: BlockNumHash,
    /// Channel that receives new blocks from the L1 node
    new_block_rx: mpsc::Receiver<Block>,
    /// The highest block that was successfully processed by the driver.
    /// We can safely prune all cached blocks below this tip once they
    /// become finalized on L1.
    processed_tip: BlockNumHash,
    /// Cache of blocks that might be reorged out. In normal conditions,
    /// this cache will not grow beyond [`FINALIZATION_TIMEOUT`] keys.
    reorg_cache: BTreeMap<BlockNumber, HashMap<B256, Block>>,
    /// Handle to the background task that fetches and processes new blocks.
    _handle: JoinHandle<()>,
}

impl StandaloneHeraContext {
    /// Create a new standalone context that polls for new chains.
    pub async fn new(l1_rpc_url: Url) -> TransportResult<Self> {
        if l1_rpc_url.scheme().contains("http") {
            debug!("Polling for new blocks via HTTP");
            Self::with_http_poller(l1_rpc_url).await
        } else if l1_rpc_url.scheme().contains("ws") {
            debug!("Subscribing to new blocks via websocket");
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

        let _handle = match client.watch_blocks().await {
            Ok(new_block_hashes) => tokio::spawn(async move {
                let mut stream = new_block_hashes.into_stream();
                while let Some(hashes) = stream.next().await {
                    for hash in hashes {
                        match client.get_block_by_hash(hash, true.into()).await {
                            Ok(Some(block)) => {
                                if let Err(e) = new_block_tx.try_send(block) {
                                    error!("Failed to send new block to channel: {:?}", e);
                                }
                            }
                            Ok(None) => {
                                // Q: does `eth_getBlockByHash` return a block if it was
                                // already reorged out? No-op for now.
                                error!("Failed to get block by hash: block not found");
                            }
                            Err(e) => {
                                error!("Failed to get block by hash: {:?}", e);
                            }
                        }
                    }
                }
            }),
            Err(err) => {
                if err
                    .as_error_resp()
                    .is_some_and(|resp| !resp.message.to_lowercase().contains("not supported"))
                {
                    // On any error other than "not supported", return the error and fail.
                    error!("Failed to watch new blocks via HTTP");
                    return Err(err);
                }

                warn!("Filtering unavailable; falling back to eth_getBlock");
                tokio::spawn(async move {
                    let mut hash = B256::ZERO;
                    loop {
                        match client.get_block(BlockId::latest(), false.into()).await {
                            Ok(Some(block)) => {
                                if hash == block.header.hash {
                                    // If the latest hash hasn't changed, wait before polling again
                                    tokio::time::sleep(Duration::from_secs(2)).await;
                                    continue;
                                }
                                hash = block.header.hash;

                                // Every time we get a new hash, fetch the full block
                                match client.get_block_by_hash(block.header.hash, true.into()).await
                                {
                                    Ok(Some(full_block)) => {
                                        if let Err(e) = new_block_tx.try_send(full_block) {
                                            error!("Failed to send new block to channel: {:?}", e);
                                        }
                                    }
                                    other => {
                                        error!("Failed to get full block by hash: {:?}", other)
                                    }
                                }
                            }
                            Ok(None) => {
                                error!("Failed to get latest block: block not found");
                            }
                            Err(e) => {
                                error!("Failed to get latest block: {:?}", e);
                            }
                        };
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                })
            }
        };

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
                if let Err(e) = new_block_tx.try_send(block) {
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
                if let Err(e) = new_block_tx.try_send(block) {
                    error!("Failed to send new block to channel: {:?}", e);
                }
            }
        });

        Ok(Self::with_defaults(new_block_rx, _handle))
    }

    /// Create a new standalone context with the given new block receiver and handle.
    fn with_defaults(new_block_rx: mpsc::Receiver<Block>, _handle: JoinHandle<()>) -> Self {
        Self {
            new_block_rx,
            _handle,
            l1_tip: BlockNumHash::default(),
            processed_tip: BlockNumHash::default(),
            reorg_cache: BTreeMap::new(),
        }
    }
}

#[async_trait]
impl DriverContext for StandaloneHeraContext {
    async fn recv_notification(&mut self) -> Option<ChainNotification> {
        // TODO: is it ok to skip fetching full txs and receipts here assuming the node will
        // have a fallback online RPC for that downstream? The driver and provider should be
        // generic but currently are very coupled to the node mode (standalone vs exex).

        let block = self.new_block_rx.recv().await?;
        let block_num = block.header.number;

        let entry = self.reorg_cache.entry(block_num).or_default();
        entry.insert(block.header.hash, block.clone());

        if block_num <= self.l1_tip.number {
            todo!("handle reorgs");
        } else {
            self.l1_tip = BlockNumHash { number: block_num, hash: block.header.hash };

            // upon a new tip, prune the reorg cache for all blocks that have been finalized,
            // as they are no longer candidates for reorgs.
            self.reorg_cache.retain(|num, _| *num > block_num - FINALIZATION_TIMEOUT);
        }

        Some(ChainNotification::New { new_blocks: Blocks::from(block) })
    }

    fn send_processed_tip_event(&mut self, tip: BlockNumHash) {
        self.processed_tip = tip;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_rpc_types::{BlockTransactions, Header};

    #[tokio::test]
    async fn test_http_poller() -> eyre::Result<()> {
        let url = Url::parse("http://localhost:8545")?;

        // skip test if we can't connect to the node
        if reqwest::get(url.clone()).await.is_err() {
            return Ok(());
        }

        let mut ctx = StandaloneHeraContext::new(url).await?;

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

        let mut ctx = StandaloneHeraContext::new(url).await?;

        let notif = ctx.recv_notification().await.unwrap();

        assert!(notif.new_chain().is_some());
        assert!(notif.reverted_chain().is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_reorg_cache_pruning() {
        let (_, rx) = mpsc::channel(128);
        let handle = tokio::spawn(async {});
        let mut ctx = StandaloneHeraContext::with_defaults(rx, handle);

        // Simulate receiving 100 blocks
        for i in 1..=100 {
            let block = create_mock_block(i);
            ctx.reorg_cache.entry(i).or_default().insert(block.header.hash, block);
        }

        // Simulate receiving a new block that should trigger pruning
        let new_block = create_mock_block(101);
        ctx.l1_tip = BlockNumHash { number: 101, ..Default::default() };
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
        let mut ctx = StandaloneHeraContext::with_defaults(rx, handle);

        // Send a processed tip event
        ctx.send_processed_tip_event(BlockNumHash { number: 100, ..Default::default() });
    }

    // Helper function to create a mock Block<TxEnvelope>
    fn create_mock_block(number: u64) -> Block {
        Block {
            header: Header { number, hash: B256::random(), ..Default::default() },
            transactions: BlockTransactions::Full(vec![]),
            uncles: vec![],
            size: None,
            withdrawals: None,
        }
    }
}
