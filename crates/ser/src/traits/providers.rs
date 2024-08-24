//! Providers for the serialization input.

use crate::types::L2BlockRef;
use alloc::boxed::Box;
use alloy::{
    primitives::Address,
    rpc::types::{Block, Header},
};
use async_trait::async_trait;
use eyre::Result;

/// A provider for the L1 Chain.
#[async_trait]
pub trait L1Provider {
    /// Fetches the header of the block with the given number.
    async fn header_by_number(&self, number: u64) -> Result<Header>;

    /// Fetches the nonce of the account at the given block number.
    async fn nonce_at(&self, account: Address, block_number: u64) -> Result<u64>;
}

/// A provider for the L2 Chain.
#[async_trait]
pub trait L2Provider {
    /// Fetches the [L2BlockRef] by number.
    async fn block_ref_by_number(&self, number: u64) -> Result<L2BlockRef>;

    /// Fetches the block with the given number.
    async fn block_by_number(&self, number: u64) -> Result<Block>;
}
