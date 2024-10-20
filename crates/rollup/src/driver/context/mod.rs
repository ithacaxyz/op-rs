//! Context abstraction for the rollup driver.

use std::{collections::BTreeMap, sync::Arc};

use alloy_eips::eip1898::BlockNumHash;
use alloy_primitives::{BlockNumber, U256};
use alloy_rpc_types::{Block, BlockTransactions, Header};
use async_trait::async_trait;
use reth_execution_types::Chain;
use reth_exex::ExExNotification;

mod exex;
pub use exex::ExExHeraContext;

mod standalone;
pub use standalone::StandaloneHeraContext;

/// Context for the rollup driver.
///
/// The context is responsible for handling notifications from the state of the
/// canonical chain updates (new blocks, reorgs, etc) and translating them into
/// events that the rollup driver can use to make progress.
#[async_trait]
pub trait DriverContext {
    /// Receives a notification from the execution client.
    async fn recv_notification(&mut self) -> Option<ChainNotification>;

    /// Sends an event indicating that the processed tip has been updated.
    fn send_processed_tip_event(&mut self, tip: BlockNumHash);
}

/// A notification representing a chain of blocks that come from an execution client.
#[derive(Debug, Clone)]
pub enum ChainNotification {
    /// A new chain of blocks has been processed.
    New { new_blocks: Blocks },
    /// Some blocks have been reverted and are no longer part of the chain.
    Revert { old_blocks: Blocks },
    /// The chain has been reorganized with new canonical blocks.
    Reorg { old_blocks: Blocks, new_blocks: Blocks },
}

impl ChainNotification {
    /// Returns the new chain of blocks contained in the notification event.
    ///
    /// For reorgs, this returns the new chain of blocks that replaced the old one.
    pub fn new_chain(&self) -> Option<Blocks> {
        match self {
            ChainNotification::New { new_blocks } => Some(new_blocks.clone()),
            ChainNotification::Reorg { new_blocks, .. } => Some(new_blocks.clone()),
            ChainNotification::Revert { .. } => None,
        }
    }

    /// Returns the old chain of blocks contained in the notification event.
    ///
    /// For reorgs, this returns the old canonical chain that was reorged.
    pub fn reverted_chain(&self) -> Option<Blocks> {
        match self {
            ChainNotification::Revert { old_blocks } => Some(old_blocks.clone()),
            ChainNotification::Reorg { old_blocks, .. } => Some(old_blocks.clone()),
            ChainNotification::New { .. } => None,
        }
    }
}

/// A collection of blocks that form a chain.
#[derive(Debug, Clone, Default)]
pub struct Blocks(Arc<BTreeMap<BlockNumber, Block>>);

impl Blocks {
    /// Returns the tip of the chain.
    pub fn tip(&self) -> BlockNumHash {
        let last = self.0.last_key_value().expect("Blocks should have at least one block").1;
        BlockNumHash::new(last.header.number, last.header.hash)
    }

    /// Returns the block at the fork point of the chain.
    pub fn fork_block_number(&self) -> BlockNumber {
        let first = self.0.first_key_value().expect("Blocks should have at least one block").0;
        first.saturating_sub(1)
    }
}

impl From<Block> for Blocks {
    fn from(value: Block) -> Self {
        let mut blocks = BTreeMap::new();
        blocks.insert(value.header.number, value);
        Self(Arc::new(blocks))
    }
}

impl From<Vec<Block>> for Blocks {
    fn from(value: Vec<Block>) -> Self {
        let mut blocks = BTreeMap::new();
        for block in value {
            blocks.insert(block.header.number, block);
        }
        Self(Arc::new(blocks))
    }
}

impl From<Arc<Chain>> for Blocks {
    fn from(value: Arc<Chain>) -> Self {
        let mut blocks = BTreeMap::new();
        for (block_number, sealed_block) in value.blocks() {
            let block = parse_reth_block_to_alloy_rpc(sealed_block.block.clone());
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

fn parse_reth_block_to_alloy_rpc(block: reth::primitives::SealedBlock) -> Block {
    let tx_hashes = block.body.transactions().map(|tx| tx.hash()).collect();

    Block {
        header: parse_reth_header_to_alloy_rpc(&block),
        uncles: block.body.ommers.iter().map(|x| x.hash_slow()).collect(),
        transactions: BlockTransactions::Hashes(tx_hashes),
        size: Some(U256::from(block.size())),
        withdrawals: block.body.withdrawals.clone().map(|w| w.into_inner()),
    }
}

// from reth::primitives::SealedBlock to alloy_rpc_types::Header
fn parse_reth_header_to_alloy_rpc(block: &reth::primitives::SealedBlock) -> Header {
    Header {
        hash: block.header.hash(),
        parent_hash: block.parent_hash,
        uncles_hash: block.ommers_hash,
        miner: block.beneficiary,
        state_root: block.state_root,
        transactions_root: block.transactions_root,
        receipts_root: block.receipts_root,
        logs_bloom: block.logs_bloom,
        difficulty: block.difficulty,
        number: block.number,
        gas_limit: block.gas_limit,
        gas_used: block.gas_used,
        timestamp: block.timestamp,
        total_difficulty: Some(block.difficulty),
        extra_data: block.extra_data.clone(),
        mix_hash: Some(block.mix_hash),
        nonce: Some(block.nonce),
        base_fee_per_gas: block.base_fee_per_gas,
        withdrawals_root: block.withdrawals_root,
        blob_gas_used: block.blob_gas_used,
        excess_blob_gas: block.excess_blob_gas,
        parent_beacon_block_root: block.parent_beacon_block_root,
        requests_hash: block.requests_hash,
    }
}
