use alloc::{collections::VecDeque, sync::Arc};
use hashbrown::HashMap;

use alloy::{eips::eip4844::Blob, primitives::B256};
use async_trait::async_trait;
use eyre::{eyre, Result};
use kona_derive::{
    errors::BlobProviderError,
    online::{
        OnlineBeaconClient, OnlineBlobProviderBuilder, OnlineBlobProviderWithFallback,
        SimpleSlotDerivation,
    },
    traits::BlobProvider,
};
use kona_primitives::IndexedBlobHash;
use op_alloy_protocol::BlockInfo;
use parking_lot::Mutex;
use reth::primitives::BlobTransactionSidecar;
use tracing::warn;
use url::Url;

/// A blob provider that first attempts to fetch blobs from a primary beacon client and
/// falls back to a secondary blob archiver if the primary fails.
///
/// Any blob archiver just needs to implement the beacon
/// [`blob_sidecars` API](https://ethereum.github.io/beacon-APIs/#/Beacon/getBlobSidecars)
pub type DurableBlobProvider =
    OnlineBlobProviderWithFallback<OnlineBeaconClient, OnlineBeaconClient, SimpleSlotDerivation>;

/// Layered [BlobProvider] for the Kona derivation pipeline.
///
/// This provider wraps different blob sources in an ordered manner:
/// - First, it attempts to fetch blobs from an in-memory store.
/// - If the blobs are not found, it then attempts to fetch them from an online beacon client.
/// - If the blobs are still not found, it tries to fetch them from a blob archiver (if set).
/// - If all sources fail, the provider will return a [BlobProviderError].
#[derive(Debug, Clone)]
pub struct LayeredBlobProvider {
    /// In-memory inner blob provider, used for locally caching blobs as
    /// they come during live sync (when following the chain tip).
    memory: Arc<Mutex<InnerBlobProvider>>,

    /// Fallback online blob provider.
    /// This is used primarily during sync when archived blobs
    /// aren't provided by reth since they'll be too old.
    ///
    /// The `Durable` setup allows to specify two different
    /// endpoints for a primary and a fallback blob provider.
    online: DurableBlobProvider,
}

/// A blob provider that hold blobs in memory.
#[derive(Debug)]
pub struct InnerBlobProvider {
    /// Maximum number of blobs to keep in memory.
    capacity: usize,
    /// Order of key insertion for oldest entry eviction.
    key_order: VecDeque<B256>,
    /// Maps block hashes to blob hashes to blob sidecars.
    blocks_to_blob_sidecars: HashMap<B256, Vec<BlobTransactionSidecar>>,
}

impl InnerBlobProvider {
    /// Creates a new [InnerBlobProvider].
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            capacity: cap,
            blocks_to_blob_sidecars: HashMap::with_capacity(cap),
            key_order: VecDeque::with_capacity(cap),
        }
    }

    /// Inserts multiple blob sidecars nto the provider.
    pub fn insert_blob_sidecars(
        &mut self,
        block_hash: B256,
        sidecars: Vec<BlobTransactionSidecar>,
    ) {
        if let Some(existing_blobs) = self.blocks_to_blob_sidecars.get_mut(&block_hash) {
            existing_blobs.extend(sidecars);
        } else {
            if self.blocks_to_blob_sidecars.len() >= self.capacity {
                if let Some(oldest) = self.key_order.pop_front() {
                    self.blocks_to_blob_sidecars.remove(&oldest);
                }
            }
            self.blocks_to_blob_sidecars.insert(block_hash, sidecars);
        }
    }
}

impl LayeredBlobProvider {
    /// Creates a new [LayeredBlobProvider] with a local blob store, an online primary beacon
    /// client and an optional fallback blob archiver for fetching blobs.
    pub fn new(beacon_client_url: Url, blob_archiver_url: Option<Url>) -> Self {
        let memory = Arc::new(Mutex::new(InnerBlobProvider::with_capacity(512)));

        let online = OnlineBlobProviderBuilder::new()
            .with_primary(beacon_client_url.to_string())
            .with_fallback(blob_archiver_url.map(|url| url.to_string()))
            .build();

        Self { memory, online }
    }

    /// Inserts multiple blob sidecars into the in-memory provider.
    #[inline]
    pub fn insert_blob_sidecars(
        &mut self,
        block_hash: B256,
        sidecars: Vec<BlobTransactionSidecar>,
    ) {
        self.memory.lock().insert_blob_sidecars(block_hash, sidecars);
    }

    /// Attempts to fetch blobs using the in-memory blob store.
    #[inline]
    async fn memory_blob_load(
        &mut self,
        block_ref: &BlockInfo,
        blob_hashes: &[IndexedBlobHash],
    ) -> Result<Vec<Blob>> {
        let locked = self.memory.lock();

        let sidecars_for_block = locked
            .blocks_to_blob_sidecars
            .get(&block_ref.hash)
            .ok_or(eyre!("No blob sidecars found for block ref: {:?}", block_ref))?;

        // for each sidecar, get the blob hashes and check if any of them are
        // part of the requested hashes. If they are, add the corresponding blobs
        // to the blobs vector.
        let mut blobs = Vec::with_capacity(blob_hashes.len());
        let requested_hashes = blob_hashes.iter().map(|h| h.hash).collect::<Vec<_>>();
        for sidecar in sidecars_for_block {
            for (hash, blob) in sidecar.versioned_hashes().zip(&sidecar.blobs) {
                if requested_hashes.contains(&hash) {
                    blobs.push(*blob);
                }
            }
        }

        Ok(blobs)
    }

    /// Attempts to fetch blobs using the online blob provider.
    #[inline]
    async fn online_blob_load(
        &mut self,
        block_ref: &BlockInfo,
        blob_hashes: &[IndexedBlobHash],
    ) -> Result<Vec<Blob>, BlobProviderError> {
        self.online.get_blobs(block_ref, blob_hashes).await
    }
}

#[async_trait]
impl BlobProvider for LayeredBlobProvider {
    /// Fetches blobs for a given block ref and the blob hashes.
    async fn get_blobs(
        &mut self,
        block_ref: &BlockInfo,
        blob_hashes: &[IndexedBlobHash],
    ) -> Result<Vec<Blob>, BlobProviderError> {
        if let Ok(b) = self.memory_blob_load(block_ref, blob_hashes).await {
            return Ok(b);
        } else {
            warn!("Blob provider falling back to online provider");
            self.online_blob_load(block_ref, blob_hashes).await
        }
    }
}
