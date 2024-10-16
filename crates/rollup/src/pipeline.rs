//! The rollup pipeline builder

use std::{fmt::Debug, sync::Arc};

use kona_derive::{
    attributes::StatefulAttributesBuilder,
    pipeline::{DerivationPipeline, PipelineBuilder},
    sources::EthereumDataSource,
    stages::{
        AttributesQueue, BatchQueue, ChannelBank, ChannelReader, FrameQueue, L1Retrieval,
        L1Traversal,
    },
    traits::BlobProvider,
};
use kona_providers::ChainProvider;
use kona_providers_alloy::AlloyL2ChainProvider;
use op_alloy_genesis::RollupConfig;
use op_alloy_protocol::BlockInfo;

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
///
/// This pipeline is a derivation pipeline that takes the outputs of the [FrameQueue] stage
/// and transforms them into
/// [OpPayloadAttributes](op_alloy_rpc_types_engine::OpPayloadAttributes).
pub type RollupPipeline<CP, BP> =
    DerivationPipeline<L1AttributesQueue<CP, BP, AlloyL2ChainProvider>, AlloyL2ChainProvider>;

/// Creates a new [RollupPipeline] from the given components.
#[allow(unused)]
pub fn new_rollup_pipeline<CP, BP>(
    cfg: Arc<RollupConfig>,
    chain_provider: CP,
    blob_provider: BP,
    l2_chain_provider: AlloyL2ChainProvider,
    origin: BlockInfo,
) -> RollupPipeline<CP, BP>
where
    CP: ChainProvider + Send + Sync + Clone + Debug,
    BP: BlobProvider + Send + Sync + Clone + Debug,
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
