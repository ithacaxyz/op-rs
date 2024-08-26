//! Execution Payload Envelope Type

use super::payload::{
    ExecutionPayloadV1SSZ, ExecutionPayloadV2SSZ, ExecutionPayloadV3SSZ, PayloadHash,
};
use alloy::primitives::{Signature, B256};
use eyre::Result;
use kona_primitives::L2ExecutionPayload;
use ssz_rs::prelude::*;
use std::str::FromStr;

/// An envelope around the execution payload for L2.
#[derive(Debug, Clone)]
pub struct ExecutionPayloadEnvelope {
    /// The execution payload.
    pub payload: L2ExecutionPayload,
    /// A signature for the payload.
    pub signature: Signature,
    /// The hash of the payload.
    pub hash: PayloadHash,
    /// The parent beacon block root.
    pub parent_beacon_block_root: Option<B256>,
}

impl ExecutionPayloadEnvelope {
    /// Decode V1
    pub fn decode_v1(data: &[u8]) -> Result<Self> {
        let mut decoder = snap::raw::Decoder::new();
        let decompressed = decoder.decompress_vec(data)?;
        let sig_data = &decompressed[..65];
        let block_data = &decompressed[65..];

        let signature = Signature::try_from(sig_data)?;

        let payload = ExecutionPayloadV1SSZ::deserialize(block_data)?;
        let payload = L2ExecutionPayload::from(payload);

        let hash = PayloadHash::from(block_data);

        Ok(ExecutionPayloadEnvelope { parent_beacon_block_root: None, signature, payload, hash })
    }

    /// Decode V2
    pub fn decode_v2(data: &[u8]) -> Result<Self> {
        let mut decoder = snap::raw::Decoder::new();
        let decompressed = decoder.decompress_vec(data)?;
        let sig_data = &decompressed[..65];
        let block_data = &decompressed[65..];

        let signature = Signature::try_from(sig_data)?;

        let payload = ExecutionPayloadV2SSZ::deserialize(block_data)?;
        let payload = L2ExecutionPayload::from(payload);

        let hash = PayloadHash::from(block_data);

        Ok(ExecutionPayloadEnvelope { parent_beacon_block_root: None, signature, payload, hash })
    }

    /// Decode V3
    pub fn decode_v3(data: &[u8]) -> Result<Self> {
        let mut decoder = snap::raw::Decoder::new();
        let decompressed = decoder.decompress_vec(data)?;
        let sig_data = &decompressed[..65];
        let parent_beacon_block_root = &decompressed[65..97];
        let block_data = &decompressed[97..];

        let signature = Signature::try_from(sig_data)?;

        let parent_beacon_block_root = Some(B256::from_slice(parent_beacon_block_root));

        let payload = ExecutionPayloadV3SSZ::deserialize(block_data)?;
        let payload = L2ExecutionPayload::from(payload);

        let hash = PayloadHash::from(block_data);

        Ok(ExecutionPayloadEnvelope { parent_beacon_block_root, signature, payload, hash })
    }
}

impl Default for ExecutionPayloadEnvelope {
    fn default() -> Self {
        Self {
            payload: L2ExecutionPayload::default(),
            // Generic signature taken from `alloy_primitives` tests.
            //
            // https://github.com/alloy-rs/core/blob/main/crates/primitives/src/signature/sig.rs#L614
            signature: Signature::from_str("48b55bfa915ac795c431978d8a6a992b628d557da5ff759b307d495a36649353efffd310ac743f371de3b9f7f9cb56c0b28ad43601b4ab949f53faa07bd2c8041b").unwrap(),
            hash: PayloadHash::default(),
            parent_beacon_block_root: None,
        }
    }
}
