use alloc::{collections::VecDeque, sync::Arc};
use async_trait::async_trait;
use hashbrown::HashMap;

use alloy::primitives::B256;
use eyre::eyre;
use kona_derive::{
    errors::BlobProviderError,
    online::{
        OnlineBeaconClient, OnlineBlobProviderBuilder, OnlineBlobProviderWithFallback,
        SimpleSlotDerivation,
    },
    traits::BlobProvider,
};
use kona_primitives::{Blob, BlockInfo, IndexedBlobHash};
use parking_lot::Mutex;
use tracing::warn;
use url::Url;

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
    /// The `WithFallback` setup allows to specify two different
    /// endpoints for a primary and a fallback blob provider.
    online: OnlineBlobProviderWithFallback<
        OnlineBeaconClient,
        OnlineBeaconClient,
        SimpleSlotDerivation,
    >,
}

/// A blob provider that hold blobs in memory.
#[derive(Debug)]
pub struct InnerBlobProvider {
    /// Maximum number of blobs to keep in memory.
    capacity: usize,
    /// Order of key insertion for oldest entry eviction.
    key_order: VecDeque<B256>,
    /// Maps block hashes to blobs.
    blocks_to_blob: HashMap<B256, Vec<Blob>>,
}

impl InnerBlobProvider {
    /// Creates a new [InnerBlobProvider].
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            capacity: cap,
            blocks_to_blob: HashMap::with_capacity(cap),
            key_order: VecDeque::with_capacity(cap),
        }
    }

    /// Inserts multiple blobs into the provider.
    pub fn insert_blobs(&mut self, block_hash: B256, blobs: Vec<Blob>) {
        if let Some(existing_blobs) = self.blocks_to_blob.get_mut(&block_hash) {
            existing_blobs.extend(blobs);
        } else {
            if self.blocks_to_blob.len() >= self.capacity {
                if let Some(oldest) = self.key_order.pop_front() {
                    self.blocks_to_blob.remove(&oldest);
                }
            }
            self.blocks_to_blob.insert(block_hash, blobs);
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

    /// Inserts multiple blobs into the in-memory provider.
    pub fn insert_blobs(&mut self, block_hash: B256, blobs: Vec<Blob>) {
        self.memory.lock().insert_blobs(block_hash, blobs);
    }

    /// Attempts to fetch blobs using the in-memory blob store.
    async fn memory_blob_load(
        &mut self,
        block_ref: &BlockInfo,
        hashes: &[IndexedBlobHash],
    ) -> eyre::Result<Vec<Blob>> {
        let locked = self.memory.lock();

        let blobs_for_block = locked
            .blocks_to_blob
            .get(&block_ref.hash)
            .ok_or_else(|| eyre!("Blob not found for block ref: {:?}", block_ref))?;

        let mut blobs = Vec::new();
        for _ in hashes {
            for blob in blobs_for_block {
                blobs.push(*blob);
            }
        }

        Ok(blobs)
    }

    /// Attempts to fetch blobs using the online blob provider.
    #[allow(unused)]
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
            self.online.get_blobs(block_ref, blob_hashes).await
        }
    }
}
