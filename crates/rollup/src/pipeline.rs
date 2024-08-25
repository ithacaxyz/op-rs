//! The rollup pipeline builder

use std::{fmt::Debug, sync::Arc};

use kona_derive::{
    online::{DerivationPipeline, EthereumDataSource, PipelineBuilder},
    stages::{
        AttributesQueue, BatchQueue, ChannelBank, ChannelReader, FrameQueue, L1Retrieval,
        L1Traversal, StatefulAttributesBuilder,
    },
    traits::{BlobProvider, ChainProvider, L2ChainProvider},
};
use kona_primitives::{BlockInfo, RollupConfig};

/// A [FrameQueue] stage implementation that takes the outputs of the [L1Retrieval] stage and
/// parses it into frames, using the [L1Traversal] stage to fetch block info for each frame.
type L1FrameQueue<CP, BP> = FrameQueue<L1Retrieval<EthereumDataSource<CP, BP>, L1Traversal<CP>>>;

/// A concrete [NextAttributes](kona_derive::traits::NextAttributes) stage implementation that
/// accepts batches from the [BatchQueue] stage and transforms them into payload attributes.
type L1AttributesQueue<CP, BP, L2CP> = AttributesQueue<
    BatchQueue<ChannelReader<ChannelBank<L1FrameQueue<CP, BP>>>, L2CP>,
    StatefulAttributesBuilder<CP, L2CP>,
>;

/// A derivation pipeline generic over:
/// - The L1 [ChainProvider] (CP)
/// - The L1 [BlobProvider] (BP)
/// - The [L2ChainProvider] (L2CP)
///
/// This pipeline is a derivation pipeline that takes the outputs of the [L1FrameQueue] stage
/// and transforms them into [L2PayloadAttributes](kona_primitives::L2PayloadAttributes).
pub type RollupPipeline<CP, BP, L2CP> = DerivationPipeline<L1AttributesQueue<CP, BP, L2CP>, L2CP>;

/// Creates a new [RollupPipeline] from the given components.
#[allow(unused)]
pub fn new_rollup_pipeline<CP, BP, L2CP>(
    cfg: Arc<RollupConfig>,
    chain_provider: CP,
    blob_provider: BP,
    l2_chain_provider: L2CP,
    origin: BlockInfo,
) -> RollupPipeline<CP, BP, L2CP>
where
    CP: ChainProvider + Send + Sync + Clone + Debug,
    BP: BlobProvider + Send + Sync + Clone + Debug,
    L2CP: L2ChainProvider + Send + Sync + Clone + Debug,
{
    let dap = EthereumDataSource::new(chain_provider.clone(), blob_provider.clone(), &cfg.clone());
    let attributes = StatefulAttributesBuilder::new(
        cfg.clone(),
        l2_chain_provider.clone(),
        chain_provider.clone(),
    );

    PipelineBuilder::new()
        .rollup_config(cfg)
        .dap_source(dap)
        .l2_chain_provider(l2_chain_provider)
        .chain_provider(chain_provider)
        .builder(attributes)
        .origin(origin)
        .build()
}
