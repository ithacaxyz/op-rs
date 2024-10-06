//! Execution Payload Envelope Type

use alloy::{
    primitives::{Signature, B256},
    rpc::types::engine::{ExecutionPayloadV1, ExecutionPayloadV2, ExecutionPayloadV3},
};
use eyre::Result;

use super::payload::PayloadHash;

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

        let payload =
            ExecutionPayloadV1::from_ssz_bytes(block_data).map_err(|e| eyre::eyre!("{:?}", e))?;
        let payload = Self::convert_payload_v1(payload);

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

        let payload =
            ExecutionPayloadV2::from_ssz_bytes(block_data).map_err(|e| eyre::eyre!("{:?}", e))?;
        let payload = Self::convert_payload_v2(payload);

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

        let payload =
            ExecutionPayloadV3::from_ssz_bytes(block_data).map_err(|e| eyre::eyre!("{:?}", e))?;
        let payload = Self::convert_payload_v3(payload);

        let hash = PayloadHash::from(block_data);

        Ok(ExecutionPayloadEnvelope { parent_beacon_block_root, signature, payload, hash })
    }

    fn convert_payload_v1(payload: ExecutionPayloadV1) -> L2ExecutionPayload {
        L2ExecutionPayload {
            parent_hash: payload.parent_hash,
            fee_recipient: payload.fee_recipient,
            state_root: payload.state_root,
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom,
            prev_randao: payload.prev_randao,
            block_number: payload.block_number,
            gas_limit: payload.gas_limit.into(),
            gas_used: payload.gas_used.into(),
            timestamp: payload.timestamp,
            extra_data: payload.extra_data,
            base_fee_per_gas: Self::convert_uint128(payload.base_fee_per_gas),
            block_hash: payload.block_hash,
            transactions: payload.transactions,
            deserialized_transactions: Vec::new(),
            withdrawals: None,
            blob_gas_used: None,
            excess_blob_gas: None,
        }
    }

    fn convert_payload_v2(payload: ExecutionPayloadV2) -> L2ExecutionPayload {
        L2ExecutionPayload {
            parent_hash: payload.payload_inner.parent_hash,
            fee_recipient: payload.payload_inner.fee_recipient,
            state_root: payload.payload_inner.state_root,
            receipts_root: payload.payload_inner.receipts_root,
            logs_bloom: payload.payload_inner.logs_bloom,
            prev_randao: payload.payload_inner.prev_randao,
            block_number: payload.payload_inner.block_number,
            gas_limit: payload.payload_inner.gas_limit.into(),
            gas_used: payload.payload_inner.gas_used.into(),
            timestamp: payload.payload_inner.timestamp,
            extra_data: payload.payload_inner.extra_data,
            base_fee_per_gas: Self::convert_uint128(payload.payload_inner.base_fee_per_gas),
            block_hash: payload.payload_inner.block_hash,
            transactions: payload.payload_inner.transactions,
            deserialized_transactions: Vec::default(),
            withdrawals: Some(Vec::new()),
            blob_gas_used: None,
            excess_blob_gas: None,
        }
    }

    fn convert_payload_v3(payload: ExecutionPayloadV3) -> L2ExecutionPayload {
        L2ExecutionPayload {
            parent_hash: payload.payload_inner.payload_inner.parent_hash,
            fee_recipient: payload.payload_inner.payload_inner.fee_recipient,
            state_root: payload.payload_inner.payload_inner.state_root,
            receipts_root: payload.payload_inner.payload_inner.receipts_root,
            logs_bloom: payload.payload_inner.payload_inner.logs_bloom,
            prev_randao: payload.payload_inner.payload_inner.prev_randao,
            block_number: payload.payload_inner.payload_inner.block_number,
            gas_limit: payload.payload_inner.payload_inner.gas_limit.into(),
            gas_used: payload.payload_inner.payload_inner.gas_used.into(),
            timestamp: payload.payload_inner.payload_inner.timestamp,
            extra_data: payload.payload_inner.payload_inner.extra_data,
            base_fee_per_gas: Self::convert_uint128(
                payload.payload_inner.payload_inner.base_fee_per_gas,
            ),
            block_hash: payload.payload_inner.payload_inner.block_hash,
            transactions: payload.payload_inner.payload_inner.transactions,
            deserialized_transactions: Vec::default(),
            withdrawals: Some(Vec::new()),
            blob_gas_used: Some(payload.blob_gas_used.into()),
            excess_blob_gas: Some(payload.excess_blob_gas.into()),
        }
    }

    fn convert_uint128(value: alloy::primitives::U256) -> Option<u128> {
        let bytes = value.to_le_bytes_vec();
        let bytes: [u8; 16] = bytes.try_into().ok()?;
        Some(u128::from_le_bytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::hex;

    #[test]
    fn decode_v1() {
        let data = hex::decode("0xbd04f043128457c6ccf35128497167442bcc0f8cce78cda8b366e6a12e526d938d1e4c1046acffffbfc542a7e212bb7d80d3a4b2f84f7b196d935398a24eb84c519789b401000000fe0300fe0300fe0300fe0300fe0300fe0300a203000c4a8fd56621ad04fc0101067601008ce60be0005b220117c32c0f3b394b346c2aa42cfa8157cd41f891aa0bec485a62fc010000").unwrap();
        let payload_envelop = ExecutionPayloadEnvelope::decode_v1(&data).unwrap();
        assert_eq!(1725271882, payload_envelop.payload.timestamp);
    }

    #[test]
    fn decode_v2() {
        let data = hex::decode("0xc104f0433805080eb36c0b130a7cc1dc74c3f721af4e249aa6f61bb89d1557143e971bb738a3f3b98df7c457e74048e9d2d7e5cd82bb45e3760467e2270e9db86d1271a700000000fe0300fe0300fe0300fe0300fe0300fe0300a203000c6b89d46525ad000205067201009cda69cb5b9b73fc4eb2458b37d37f04ff507fe6c9cd2ab704a05ea9dae3cd61760002000000020000").unwrap();
        let payload_envelop = ExecutionPayloadEnvelope::decode_v2(&data).unwrap();
        assert_eq!(1708427627, payload_envelop.payload.timestamp);
    }

    #[test]
    fn decode_v3() {
        let data = hex::decode("0xf104f0434442b9eb38b259f5b23826e6b623e829d2fb878dac70187a1aecf42a3f9bedfd29793d1fcb5822324be0d3e12340a95855553a65d64b83e5579dffb31470df5d010000006a03000412346a1d00fe0100fe0100fe0100fe0100fe0100fe01004201000cc588d465219504100201067601007cfece77b89685f60e3663b6e0faf2de0734674eb91339700c4858c773a8ff921e014401043e0100").unwrap();
        let payload_envelop = ExecutionPayloadEnvelope::decode_v3(&data).unwrap();
        assert_eq!(1708427461, payload_envelop.payload.timestamp);
    }
}
