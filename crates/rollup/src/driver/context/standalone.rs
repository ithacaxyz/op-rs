use futures::StreamExt;
use hashbrown::HashMap;
use std::collections::BTreeMap;

use alloy::{
    network::Ethereum,
    primitives::{BlockNumber, B256},
    providers::{Provider, ReqwestProvider},
    rpc::types::Block,
    transports::{TransportErrorKind, TransportResult},
};
use async_trait::async_trait;
use kona_primitives::TxEnvelope;
use reth::rpc::types::{BlockTransactions, BlockTransactionsKind};
use reth_exex::ExExEvent;
use tokio::{
    sync::mpsc::{self, error::SendError},
    task::JoinHandle,
};
use tracing::{debug, error};
use url::Url;

use super::{Blocks, ChainNotification, DriverContext};

/// The number of blocks to keep in the reorg cache.
/// Equivalent to 2 epochs at 32 slots/epoch.
const FINALIZATION_BLOCK_THRESHOLD: u64 = 64;

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
    /// The channel to receive processed block numbers from the driver
    processed_block_rx: Option<mpsc::Receiver<BlockNumber>>,
    /// Cache of blocks that might be reorged out
    reorg_cache: BTreeMap<BlockNumber, HashMap<B256, Block<TxEnvelope>>>,
    /// Handle to the background task that fetches and processes new blocks.
    _handle: JoinHandle<()>,
}

impl StandaloneContext {
    /// Create a new standalone context that polls for new chains.
    pub async fn new(l1_rpc_url: Url) -> TransportResult<Self> {
        let client = ReqwestProvider::<Ethereum>::new_http(l1_rpc_url.clone());

        if l1_rpc_url.scheme().contains("http") {
            debug!("Polling for new blocks via HTTP");
            Self::with_http_poller(client).await
        } else if l1_rpc_url.scheme().contains("ws") || l1_rpc_url.scheme().contains("file") {
            debug!("Subscribing to new blocks via websocket/ipc");
            Self::with_ws_subscriber(client).await
        } else {
            Err(TransportErrorKind::custom_str("Unsupported URL scheme"))
        }
    }

    /// Create a new standalone context that polls for new blocks via HTTP.
    async fn with_http_poller(client: ReqwestProvider<Ethereum>) -> TransportResult<Self> {
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
    async fn with_ws_subscriber(client: ReqwestProvider<Ethereum>) -> TransportResult<Self> {
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
        Self {
            new_block_rx,
            _handle,
            l1_tip: 0,
            processed_tip: 0,
            processed_block_rx: None,
            reorg_cache: BTreeMap::new(),
        }
    }
}

#[async_trait]
impl DriverContext for StandaloneContext {
    async fn recv_notification(&mut self) -> Option<ChainNotification> {
        // TODO: is it ok to skip fetching full txs and receipts here assuming the node will
        // have a fallback online RPC for that downstream? The driver and provider should be
        // generic but currently are very coupled to the node mode (standalone vs exex).

        let block = self.new_block_rx.recv().await?;
        let block_num = block.header.number.unwrap_or(0);

        let entry = self.reorg_cache.entry(block_num).or_default();
        entry.insert(block.header.hash.unwrap(), block.clone());

        if block_num <= self.l1_tip {
            todo!("handle reorgs");
        } else {
            // upon a new tip, prune the reorg cache for all blocks that have been finalized,
            // as they are no longer candidates for reorgs.
            self.reorg_cache.retain(|num, _| *num > block_num - FINALIZATION_BLOCK_THRESHOLD);
        }

        Some(ChainNotification::New { new_blocks: Blocks::from(block) })
    }

    fn send_event(&mut self, _event: ExExEvent) -> Result<(), SendError<ExExEvent>> {
        // TODO: When we have a better background notifier abstraction, sending
        // FinishedHeight events will be useful to make the pipeline advance
        // safely through reorgs (as it is currently done for ExExContext).
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
        other: block.other,
    }
}
