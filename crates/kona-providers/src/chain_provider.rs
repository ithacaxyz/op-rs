//! Chain Provider

use alloc::{collections::vec_deque::VecDeque, sync::Arc};
use hashbrown::HashMap;

use async_trait::async_trait;
use eyre::Result;
use kona_derive::{
    traits::ChainProvider,
    types::{
        alloy_primitives::B256, BlockID, BlockInfo, Header, Receipt, Signed, TxEip1559, TxEip2930,
        TxEip4844, TxEip4844Variant, TxEnvelope, TxLegacy,
    },
};
use parking_lot::RwLock;
use reth::{primitives::Transaction, providers::Chain};

/// An in-memory [ChainProvider] that stores chain data,
/// meant to be shared between multiple readers.
///
/// This provider is implemented with `FIFOMap`s to avoid
/// storing an unbounded amount of data.
#[derive(Debug, Clone)]
pub struct InMemoryChainProvider(Arc<RwLock<InMemoryChainProviderInner>>);

impl InMemoryChainProvider {
    /// Create a new [InMemoryChainProvider] with the given capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self(Arc::new(RwLock::new(InMemoryChainProviderInner::with_capacity(cap))))
    }

    /// Commits Chain state to the provider.
    pub fn commit(&mut self, chain: Arc<Chain>) {
        self.0.write().commit(chain);
    }

    /// Inserts the L2 genesis [BlockID] into the provider.
    pub fn insert_l2_genesis_block(&mut self, block: BlockID) {
        self.0.write().insert_l2_genesis_block(block);
    }
}

/// The inner state of an [InMemoryChainProvider].
#[derive(Debug)]
pub struct InMemoryChainProviderInner {
    /// The maximum number of items to store in the provider.
    /// This is used to prevent unbounded memory usage.
    capacity: usize,

    /// The order in which keys were inserted into the provider.
    /// This is used to evict the oldest items when the provider
    /// reaches its capacity.
    key_order: VecDeque<B256>,

    /// Maps [B256] hash to [Header].
    hash_to_header: HashMap<B256, Header>,

    /// Maps [B256] hash to [BlockInfo].
    hash_to_block_info: HashMap<B256, BlockInfo>,

    /// Maps [B256] hash to [Vec]<[Receipt]>.
    hash_to_receipts: HashMap<B256, Vec<Receipt>>,

    /// Maps a [B256] hash to a [Vec]<[TxEnvelope]>.
    hash_to_txs: HashMap<B256, Vec<TxEnvelope>>,
}
