//! Attributes validator for the rollup node

use std::fmt::Debug;

use alloy::{
    eips::BlockNumberOrTag,
    network::AnyNetwork,
    primitives::Bytes,
    providers::{
        network::primitives::BlockTransactionsKind, Provider, ReqwestProvider, RootProvider,
    },
    rpc::{client::RpcClient, types::engine::PayloadAttributes},
};
use alloy_transport_http::{
    hyper_util::{
        client::legacy::{connect::HttpConnector, Client},
        rt::TokioExecutor,
    },
    AuthLayer, AuthService, Http, HyperClient,
};
use async_trait::async_trait;
use eyre::{bail, eyre, Result};
use http_body_util::Full;
use op_alloy_provider::ext::engine::OpEngineApi;
use op_alloy_rpc_types_engine::{OpAttributesWithParent, OpPayloadAttributes};
use reth::rpc::types::{
    engine::{ForkchoiceState, JwtSecret},
    Header,
};
use tower::ServiceBuilder;
use tracing::{error, warn};
use url::Url;

/// AttributesValidator
///
/// A trait that defines the interface for validating newly derived L2 attributes.
#[async_trait]
pub trait AttributesValidator: Debug + Send {
    /// Validates the given [`OpAttributesWithParent`] and returns true
    /// if the attributes are valid, false otherwise.
    async fn validate(&self, attributes: &OpAttributesWithParent) -> Result<bool>;
}

/// TrustedValidator
///
/// Validates the [`OpAttributesWithParent`] by fetching the associated L2 block from
/// a trusted L2 RPC and constructing the L2 Attributes from the block.
#[derive(Debug, Clone)]
pub struct TrustedValidator {
    /// The L2 provider.
    provider: ReqwestProvider,
    /// The canyon activation timestamp.
    canyon_activation: u64,
}

impl TrustedValidator {
    /// Creates a new [`TrustedValidator`].
    pub fn new(provider: ReqwestProvider, canyon_activation: u64) -> Self {
        Self { provider, canyon_activation }
    }

    /// Creates a new [`TrustedValidator`] from the provided [Url].
    #[allow(unused)]
    pub fn new_http(url: Url, canyon_activation: u64) -> Self {
        let inner = ReqwestProvider::new_http(url);
        Self::new(inner, canyon_activation)
    }

    /// Fetches a block [Header] and a list of raw RLP encoded transactions from the L2 provider.
    ///
    /// This method needs to fetch the non-hydrated block and then
    /// fetch the raw transactions using the `debug_*` namespace.
    pub async fn get_block(&self, tag: BlockNumberOrTag) -> Result<(Header, Vec<Bytes>)> {
        // Don't hydrate the block so we only get a list of transaction hashes.
        let block = self
            .provider
            .get_block(tag.into(), BlockTransactionsKind::Hashes)
            .await
            .map_err(|e| eyre!(format!("Failed to fetch block: {:?}", e)))?
            .ok_or(eyre!("Block not found"))?;

        // For each transaction hash, fetch the raw transaction RLP.
        let mut txs = vec![];
        for tx in block.transactions.hashes() {
            match self.provider.raw_request("debug_getRawTransaction".into(), [tx]).await {
                Ok(tx) => txs.push(tx),
                Err(err) => {
                    error!(?err, "Failed to fetch RLP transaction");
                    bail!("Failed to fetch transaction");
                }
            }
        }

        // sanity check that we fetched all transactions
        if txs.len() != block.transactions.len() {
            bail!("Transaction count mismatch");
        }

        Ok((block.header, txs))
    }

    /// Gets the payload for the specified [BlockNumberOrTag].
    pub async fn get_payload(&self, tag: BlockNumberOrTag) -> Result<OpPayloadAttributes> {
        let (header, transactions) = self.get_block(tag).await?;

        Ok(OpPayloadAttributes {
            payload_attributes: PayloadAttributes {
                timestamp: header.timestamp,
                suggested_fee_recipient: header.miner,
                prev_randao: header.mix_hash.unwrap_or_default(),
                // Withdrawals on optimism are always empty, *after* canyon (Shanghai) activation
                withdrawals: (header.timestamp >= self.canyon_activation).then_some(Vec::default()),
                parent_beacon_block_root: header.parent_beacon_block_root,
            },
            transactions: Some(transactions),
            no_tx_pool: Some(true),
            gas_limit: Some(header.gas_limit),
            eip_1559_params: None, // TODO: fix this
        })
    }
}

#[async_trait]
impl AttributesValidator for TrustedValidator {
    async fn validate(&self, attributes: &OpAttributesWithParent) -> Result<bool> {
        let expected = attributes.parent.block_info.number + 1;
        let tag = BlockNumberOrTag::from(expected);

        match self.get_payload(tag).await {
            Ok(payload) => Ok(attributes.attributes == payload),
            Err(err) => {
                error!(?err, "Failed to fetch payload for block {}", expected);
                bail!("Failed to fetch payload for block {}: {:?}", expected, err);
            }
        }
    }
}

/// A hyper client with a JWT auth layer middleware.
type HyperAuthClient<B = Full<Bytes>> = HyperClient<B, AuthService<Client<HttpConnector, B>>>;

/// EngineApiValidator
///
/// Validates the [`OpAttributesWithParent`] by sending the attributes to an L2 engine API.
/// The engine API will return a `VALID` or `INVALID` response.
#[derive(Debug, Clone)]
pub struct EngineApiValidator {
    provider: RootProvider<Http<HyperAuthClient>, AnyNetwork>,
}

impl EngineApiValidator {
    /// Creates a new [`EngineApiValidator`] from the provided [Url] and [JwtSecret].
    pub fn new_http(url: Url, jwt: JwtSecret) -> Self {
        let hyper_client = Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>();

        let auth_layer = AuthLayer::new(jwt);
        let service = ServiceBuilder::new().layer(auth_layer).service(hyper_client);

        let layer_transport = HyperClient::with_service(service);
        let http_hyper = Http::with_client(layer_transport, url);
        let rpc_client = RpcClient::new(http_hyper, true);
        let provider = RootProvider::<_, AnyNetwork>::new(rpc_client);

        Self { provider }
    }
}

#[async_trait]
impl AttributesValidator for EngineApiValidator {
    async fn validate(&self, attributes: &OpAttributesWithParent) -> Result<bool> {
        // TODO: use the correct values
        let fork_choice_state = ForkchoiceState {
            head_block_hash: attributes.parent.block_info.hash,
            finalized_block_hash: attributes.parent.block_info.hash,
            safe_block_hash: attributes.parent.block_info.hash,
        };

        let attributes = Some(attributes.attributes.clone());
        let fcu = self.provider.fork_choice_updated_v2(fork_choice_state, attributes).await?;

        if fcu.is_valid() {
            Ok(true)
        } else {
            warn!(status = %fcu.payload_status, "Engine API returned invalid fork choice update");
            Ok(false)
        }
    }
}
