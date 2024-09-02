use std::{collections::BTreeMap, sync::Arc};

use alloy::{
    consensus::TxEnvelope,
    primitives::{BlockNumber, U256},
    rpc::types::Block,
};
use async_trait::async_trait;
use kona_providers::chain_provider::reth_to_alloy_tx;
use reth::rpc::types::{BlockTransactions, Header, OtherFields};
use reth_execution_types::Chain;
use reth_exex::{ExExEvent, ExExNotification};
use tokio::sync::mpsc::error::SendError;

mod exex;

mod standalone;
pub use standalone::StandaloneContext;

#[async_trait]
pub trait DriverContext {
    async fn recv_notification(&mut self) -> Option<ChainNotification>;

    fn send_event(&mut self, event: ExExEvent) -> Result<(), SendError<ExExEvent>>;
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
    fn from(value: Block<TxEnvelope>) -> Self {
        let mut blocks = BTreeMap::new();
        blocks.insert(value.header.number.unwrap(), value);
        Self(Arc::new(blocks))
    }
}

impl From<Vec<Block<TxEnvelope>>> for Blocks {
    fn from(value: Vec<Block<TxEnvelope>>) -> Self {
        let mut blocks = BTreeMap::new();
        for block in value {
            blocks.insert(block.header.number.unwrap(), block);
        }
        Self(Arc::new(blocks))
    }
}

impl From<Arc<Chain>> for Blocks {
    fn from(value: Arc<Chain>) -> Self {
        // annoyingly, we must convert from reth::primitives::SealedBlockWithSenders
        // to alloy::rpc::types::Block

        let mut blocks = BTreeMap::new();
        for (block_number, sealed_block) in value.blocks() {
            let block = Block {
                header: parse_reth_rpc_header(sealed_block),
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

// from reth::primitives::SealedBlock to alloy::rpc::types::Header
fn parse_reth_rpc_header(block: &reth::primitives::SealedBlock) -> Header {
    Header {
        hash: Some(block.header.hash()),
        parent_hash: block.parent_hash,
        uncles_hash: block.ommers_hash,
        miner: block.beneficiary,
        state_root: block.state_root,
        transactions_root: block.transactions_root,
        receipts_root: block.receipts_root,
        logs_bloom: block.logs_bloom,
        difficulty: block.difficulty,
        number: Some(block.number),
        gas_limit: block.gas_limit as u128,
        gas_used: block.gas_used as u128,
        timestamp: block.timestamp,
        total_difficulty: Some(block.difficulty),
        extra_data: block.extra_data.clone(),
        mix_hash: Some(block.mix_hash),
        nonce: Some(block.nonce.into()),
        base_fee_per_gas: block.base_fee_per_gas.map(|x| x as u128),
        withdrawals_root: block.withdrawals_root,
        blob_gas_used: block.blob_gas_used.map(|x| x as u128),
        excess_blob_gas: block.excess_blob_gas.map(|x| x as u128),
        parent_beacon_block_root: block.parent_beacon_block_root,
        requests_root: block.requests_root,
    }
}
