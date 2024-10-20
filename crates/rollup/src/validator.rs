//! Attributes validator for the rollup node

use std::fmt::Debug;

use alloy_eips::BlockNumberOrTag;
use alloy_primitives::Bytes;
use alloy_provider::{network::primitives::BlockTransactionsKind, Provider, ReqwestProvider};
use alloy_rpc_types_engine::PayloadAttributes;
use alloy_rpc_types_eth::Header;

use eyre::{bail, eyre, Result};
use op_alloy_rpc_types_engine::{OpAttributesWithParent, OpPayloadAttributes};
use tracing::error;
use url::Url;

/// Trusted node client that validates the [`OpAttributesWithParent`] by fetching the associated L2
/// block from a trusted L2 RPC and constructing the L2 Attributes from the block.
#[derive(Debug, Clone)]
pub struct TrustedPayloadValidator {
    /// The L2 provider.
    provider: ReqwestProvider,
    /// The canyon activation timestamp.
    canyon_activation: u64,
}

impl TrustedPayloadValidator {
    /// Creates a new [`TrustedPayloadValidator`].
    pub fn new(provider: ReqwestProvider, canyon_activation: u64) -> Self {
        Self { provider, canyon_activation }
    }

    /// Creates a new [`TrustedPayloadValidator`] from the provided [Url].
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

    /// Validates the [`OpAttributesWithParent`] by fetching the associated L2 block from
    /// a trusted L2 RPC and constructing the L2 Attributes from the block.
    pub async fn validate_payload(&self, attributes: &OpAttributesWithParent) -> Result<bool> {
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
