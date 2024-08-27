//! Execution Payload Envelope Type

use alloy::primitives::{Signature, B256};
use eyre::Result;
use kona_primitives::L2ExecutionPayload;
use ssz_rs::prelude::*;

use super::payload::{
    ExecutionPayloadV1SSZ, ExecutionPayloadV2SSZ, ExecutionPayloadV3SSZ, PayloadHash,
};

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
