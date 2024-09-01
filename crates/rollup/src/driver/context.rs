use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use alloy::{
    primitives::{BlockNumber, B256, U256},
    providers::{Provider, ReqwestProvider},
    rpc::types::Block,
};
use async_trait::async_trait;
use futures::StreamExt;
use kona_primitives::TxEnvelope;
use kona_providers::chain_provider::reth_to_alloy_tx;
use reth::rpc::types::{BlockTransactions, BlockTransactionsKind, Header, OtherFields};
use reth_execution_types::Chain;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use tokio::{
    sync::mpsc::{self, error::SendError},
    task::JoinHandle,
};
use tracing::{debug, error, warn};
use url::Url;

#[async_trait]
pub trait DriverContext {
    async fn recv_notification(&mut self) -> Option<ChainNotification>;

    fn send_event(&mut self, event: ExExEvent) -> Result<(), SendError<ExExEvent>>;
}

#[async_trait]
impl<N: FullNodeComponents> DriverContext for ExExContext<N> {
    async fn recv_notification(&mut self) -> Option<ChainNotification> {
        self.notifications.recv().await.map(ChainNotification::from)
    }

    fn send_event(&mut self, event: ExExEvent) -> Result<(), SendError<ExExEvent>> {
        self.events.send(event)
    }
}

const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[allow(unused)]
#[derive(Debug)]
pub struct StandaloneContext {
    /// The current tip of the L1 chain listener, used to detect reorgs
    l1_tip: u64,
    /// Channel that receives new blocks from the L1 node
    new_block_rx: mpsc::Receiver<Block<TxEnvelope>>,
    /// The highest block that was successfully processed by the driver.
    /// We can safely prune all cached blocks below this tip once they
    /// become finalized on L1.
    processed_tip: u64,
    /// The channel to receive processed block numbers from the driver
    processed_block_rx: Option<mpsc::Receiver<BlockNumber>>,
    /// Cache of blocks that might be reorged out
    reorg_cache: VecDeque<Block>,
    /// Handle to the background task that fetches and processes new blocks.
    _handle: JoinHandle<()>,
}

impl StandaloneContext {
    /// Create a new standalone context that polls for new chains.
    pub async fn new(l1_rpc_url: Url) -> eyre::Result<Self> {
        let client = ReqwestProvider::<alloy::network::Ethereum>::new_http(l1_rpc_url.clone());
        let (new_block_tx, new_block_rx) = mpsc::channel(128);

        if l1_rpc_url.scheme().contains("http") {
            debug!("Polling for new blocks via HTTP");

            let poller = client.watch_blocks().await?;
            let mut new_block_hashes = poller.with_poll_interval(POLL_INTERVAL).into_stream();

            let _handle = tokio::spawn(async move {
                while let Some(hashes) = new_block_hashes.next().await {
                    for hash in hashes {
                        match client.get_block_by_hash(hash, BlockTransactionsKind::Full).await {
                            Ok(Some(block)) => {
                                let txs = block
                                    .transactions
                                    .as_transactions()
                                    .unwrap_or_default()
                                    .iter()
                                    .cloned()
                                    .flat_map(|tx| TxEnvelope::try_from(tx).ok())
                                    .collect::<Vec<_>>();

                                let block_with_txs = Block {
                                    header: block.header,
                                    uncles: block.uncles,
                                    transactions: BlockTransactions::Full(txs),
                                    size: block.size,
                                    withdrawals: block.withdrawals,
                                    other: block.other,
                                };

                                if let Err(e) = new_block_tx.try_send(block_with_txs) {
                                    error!("Failed to send new block to channel: {:?}", e);
                                }
                            }
                            Ok(None) => {
                                // Question: does `eth_getBlockByHash` return a block if it was
                                // already reorged out? No-op for now.
                                warn!("Block {hash} not found");
                            }
                            Err(e) => {
                                error!("Failed to get block by hash: {:?}", e);
                            }
                        }
                    }
                }
            });

            Ok(Self::with_defaults(new_block_rx, _handle))
        } else {
            debug!("Subscribing to new blocks via websocket");
            let mut block_sub = client.subscribe_blocks().await?.into_stream();

            let _handle = tokio::spawn(async move {
                while let Some(block) = block_sub.next().await {
                    let block_with_txs_decoded = parse_block_with_transactions_decoded(block);
                    if let Err(e) = new_block_tx.try_send(block_with_txs_decoded) {
                        error!("Failed to send new block to channel: {:?}", e);
                    }
                }
            });

            Ok(Self::with_defaults(new_block_rx, _handle))
        }
    }

    fn with_defaults(
        new_block_rx: mpsc::Receiver<Block<TxEnvelope>>,
        _handle: JoinHandle<()>,
    ) -> Self {
        Self {
            new_block_rx,
            _handle,
            reorg_cache: VecDeque::new(),
            l1_tip: 0,
            processed_tip: 0,
            processed_block_rx: None,
        }
    }
}

#[async_trait]
impl DriverContext for StandaloneContext {
    async fn recv_notification(&mut self) -> Option<ChainNotification> {
        // TODO: handle reorgs.

        // TODO: is it ok to skip fetching full txs and receipts here assuming the node will
        // have a fallback online RPC for that downstream? The driver and provider should be
        // generic but currently are very coupled to the node mode (standalone vs exex).

        let block = self.new_block_rx.recv().await?;

        if block.header.number.unwrap_or(0) <= self.l1_tip {
            unimplemented!("Reorg handling not implemented yet");
        }

        let new_blocks = Blocks::default();
        Some(ChainNotification::New { new_blocks })
    }

    fn send_event(&mut self, _event: ExExEvent) -> Result<(), SendError<ExExEvent>> {
        // TODO: When we have a better background notifier abstraction, sending
        // FinishedHeight events will be useful to make the pipeline advance
        // safely through reorgs (as it is currently done for ExExContext).
        Ok(())
    }
}

/// A simple notification type that represents a chain of blocks, without
/// any EVM execution results or state.
pub enum ChainNotification {
    New { new_blocks: Blocks },
    Revert { old_blocks: Blocks },
    Reorg { old_blocks: Blocks, new_blocks: Blocks },
}

impl ChainNotification {
    pub fn new_chain(&self) -> Option<Blocks> {
        match self {
            ChainNotification::New { new_blocks } => Some(new_blocks.clone()),
            ChainNotification::Reorg { new_blocks, .. } => Some(new_blocks.clone()),
            ChainNotification::Revert { .. } => None,
        }
    }

    pub fn reverted_chain(&self) -> Option<Blocks> {
        match self {
            ChainNotification::Revert { old_blocks } => Some(old_blocks.clone()),
            ChainNotification::Reorg { old_blocks, .. } => Some(old_blocks.clone()),
            ChainNotification::New { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Blocks(Arc<BTreeMap<BlockNumber, Block<TxEnvelope>>>);

impl Blocks {
    pub fn tip(&self) -> BlockNumber {
        *self.0.last_key_value().expect("Blocks should have at least one block").0
    }

    pub fn fork_block(&self) -> BlockNumber {
        let first = self.0.first_key_value().expect("Blocks should have at least one block").0;
        first.saturating_sub(1)
    }
}

impl From<Block<TxEnvelope>> for Blocks {
    fn from(block: Block<TxEnvelope>) -> Self {
        let mut blocks = BTreeMap::new();
        blocks.insert(block.header.number.unwrap(), block);
        Self(Arc::new(blocks))
    }
}

impl From<Arc<Chain>> for Blocks {
    fn from(value: Arc<Chain>) -> Self {
        // annoyingly, we must convert from reth::primitives::SealedBlockWithSenders
        // to alloy::rpc::types::Block
        // :(

        let mut blocks = BTreeMap::new();
        for (block_number, sealed_block) in value.blocks() {
            let block = Block {
                header: this_should_not_exist(sealed_block.header.hash(), &sealed_block.header),
                uncles: sealed_block.ommers.iter().map(|x| x.hash_slow()).collect(),
                transactions: BlockTransactions::Full(
                    sealed_block.transactions().flat_map(reth_to_alloy_tx).collect(),
                ),
                size: Some(U256::from(sealed_block.size())),
                withdrawals: None, // TODO: parse withdrawals
                other: OtherFields::default(),
            };
            blocks.insert(*block_number, block);
        }
        Self(Arc::new(blocks))
    }
}

impl From<ExExNotification> for ChainNotification {
    fn from(value: ExExNotification) -> Self {
        match value {
            ExExNotification::ChainCommitted { new } => Self::New { new_blocks: new.into() },
            ExExNotification::ChainReverted { old } => Self::Revert { old_blocks: old.into() },
            ExExNotification::ChainReorged { old, new } => {
                Self::Reorg { old_blocks: old.into(), new_blocks: new.into() }
            }
        }
    }
}

fn this_should_not_exist(hash: B256, header: &reth::primitives::Header) -> Header {
    Header {
        hash: Some(hash),
        parent_hash: header.parent_hash,
        uncles_hash: header.ommers_hash,
        miner: header.beneficiary,
        state_root: header.state_root,
        transactions_root: header.transactions_root,
        receipts_root: header.receipts_root,
        logs_bloom: header.logs_bloom,
        difficulty: header.difficulty,
        number: Some(header.number),
        gas_limit: header.gas_limit as u128,
        gas_used: header.gas_used as u128,
        timestamp: header.timestamp,
        total_difficulty: Some(header.difficulty),
        extra_data: header.extra_data.clone(),
        mix_hash: Some(header.mix_hash),
        nonce: Some(header.nonce.into()),
        base_fee_per_gas: header.base_fee_per_gas.map(|x| x as u128),
        withdrawals_root: header.withdrawals_root,
        blob_gas_used: header.blob_gas_used.map(|x| x as u128),
        excess_blob_gas: header.excess_blob_gas.map(|x| x as u128),
        parent_beacon_block_root: header.parent_beacon_block_root,
        requests_root: header.requests_root,
    }
}

fn parse_block_with_transactions_decoded(block: Block) -> Block<TxEnvelope> {
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
