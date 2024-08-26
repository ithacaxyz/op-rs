//! SSZ Types

use kona_primitives::L2ExecutionPayload;
use ssz_rs::{prelude::*, List, Vector, U256};

/// A type alias for a vector of 32 bytes, representing a Bytes32 hash
type Bytes32 = Vector<u8, 32>;
/// A type alias for a vector of 20 bytes, representing an address
type VecAddress = Vector<u8, 20>;
/// A type alias for a byte list, representing a transaction
type Transaction = List<u8, 1073741824>;

/// The pre Canyon/Shanghai [ExecutionPayload] - the withdrawals field should not exist
#[derive(SimpleSerialize, Default)]
pub struct ExecutionPayloadV1SSZ {
    /// Block hash of the parent block
    pub parent_hash: Bytes32,
    /// Fee recipient of the block. Set to the sequencer fee vault
    pub fee_recipient: VecAddress,
    /// State root of the block
    pub state_root: Bytes32,
    /// Receipts root of the block
    pub receipts_root: Bytes32,
    /// Logs bloom of the block
    pub logs_bloom: Vector<u8, 256>,
    /// The block mix_digest
    pub prev_randao: Bytes32,
    /// The block number
    pub block_number: u64,
    /// The block gas limit
    pub gas_limit: u64,
    /// Total gas used in the block
    pub gas_used: u64,
    /// Timestamp of the block
    pub timestamp: u64,
    /// Any extra data included in the block
    pub extra_data: List<u8, 32>,
    /// Base fee per gas of the block
    pub base_fee_per_gas: U256,
    /// Hash of the block
    pub block_hash: Bytes32,
    /// Transactions in the block
    pub transactions: List<Transaction, 1048576>,
}

impl From<ExecutionPayloadV1SSZ> for L2ExecutionPayload {
    fn from(value: ExecutionPayloadV1SSZ) -> Self {
        Self {
            parent_hash: convert_hash(value.parent_hash),
            fee_recipient: convert_address(value.fee_recipient),
            state_root: convert_hash(value.state_root),
            receipts_root: convert_hash(value.receipts_root),
            logs_bloom: convert_bloom(value.logs_bloom),
            prev_randao: convert_hash(value.prev_randao),
            block_number: value.block_number,
            gas_limit: value.gas_limit.into(),
            gas_used: value.gas_used.into(),
            timestamp: value.timestamp,
            extra_data: convert_byte_list(value.extra_data),
            base_fee_per_gas: convert_uint(value.base_fee_per_gas),
            block_hash: convert_hash(value.block_hash),
            transactions: convert_tx_list(value.transactions),
            deserialized_transactions: Vec::default(),
            withdrawals: None,
            blob_gas_used: None,
            excess_blob_gas: None,
        }
    }
}

/// The Canyon/Shanghai [ExecutionPayload] - the withdrawals field should be an empty [List]
#[derive(SimpleSerialize, Default)]
pub struct ExecutionPayloadV2SSZ {
    /// Block hash of the parent block
    pub parent_hash: Bytes32,
    /// Fee recipient of the block. Set to the sequencer fee vault
    pub fee_recipient: VecAddress,
    /// State root of the block
    pub state_root: Bytes32,
    /// Receipts root of the block
    pub receipts_root: Bytes32,
    /// Logs bloom of the block
    pub logs_bloom: Vector<u8, 256>,
    /// The block mix_digest
    pub prev_randao: Bytes32,
    /// The block number
    pub block_number: u64,
    /// The block gas limit
    pub gas_limit: u64,
    /// Total gas used in the block
    pub gas_used: u64,
    /// Timestamp of the block
    pub timestamp: u64,
    /// Any extra data included in the block
    pub extra_data: List<u8, 32>,
    /// Base fee per gas of the block
    pub base_fee_per_gas: U256,
    /// Hash of the block
    pub block_hash: Bytes32,
    /// Transactions in the block
    pub transactions: List<Transaction, 1048576>,
    /// An empty list. This is unused and only exists for L1 compatibility.
    pub withdrawals: List<Withdrawal, 16>,
}

/// This represents an L1 validator Withdrawal, and is unused in OP stack rollups.
/// Exists only for L1 compatibility
#[derive(SimpleSerialize, Default)]
pub struct Withdrawal {
    /// Index of the withdrawal
    index: u64,
    /// Index of the validator
    validator_index: u64,
    /// Account address that has withdrawn
    address: VecAddress,
    /// The amount withdrawn
    amount: u64,
}

impl From<ExecutionPayloadV2SSZ> for L2ExecutionPayload {
    /// Converts an ExecutionPayloadV2SSZ received via p2p gossip into an [ExecutionPayload] used by
    /// the engine.
    fn from(value: ExecutionPayloadV2SSZ) -> Self {
        Self {
            parent_hash: convert_hash(value.parent_hash),
            fee_recipient: convert_address(value.fee_recipient),
            state_root: convert_hash(value.state_root),
            receipts_root: convert_hash(value.receipts_root),
            logs_bloom: convert_bloom(value.logs_bloom),
            prev_randao: convert_hash(value.prev_randao),
            block_number: value.block_number,
            gas_limit: value.gas_limit.into(),
            gas_used: value.gas_used.into(),
            timestamp: value.timestamp,
            extra_data: convert_byte_list(value.extra_data),
            base_fee_per_gas: convert_uint(value.base_fee_per_gas),
            block_hash: convert_hash(value.block_hash),
            transactions: convert_tx_list(value.transactions),
            deserialized_transactions: Vec::default(),
            withdrawals: Some(Vec::new()),
            blob_gas_used: None,
            excess_blob_gas: None,
        }
    }
}

/// The Ecotone [ExecutionPayload] - Adds Eip 4844 fields to the payload
/// - `blob_gas_used`
/// - `excess_blob_gas`
#[derive(SimpleSerialize, Default)]
pub struct ExecutionPayloadV3SSZ {
    /// Block hash of the parent block
    pub parent_hash: Bytes32,
    /// Fee recipient of the block. Set to the sequencer fee vault
    pub fee_recipient: VecAddress,
    /// State root of the block
    pub state_root: Bytes32,
    /// Receipts root of the block
    pub receipts_root: Bytes32,
    /// Logs bloom of the block
    pub logs_bloom: Vector<u8, 256>,
    /// The block mix_digest
    pub prev_randao: Bytes32,
    /// The block number
    pub block_number: u64,
    /// The block gas limit
    pub gas_limit: u64,
    /// Total gas used in the block
    pub gas_used: u64,
    /// Timestamp of the block
    pub timestamp: u64,
    /// Any extra data included in the block
    pub extra_data: List<u8, 32>,
    /// Base fee per gas of the block
    pub base_fee_per_gas: U256,
    /// Hash of the block
    pub block_hash: Bytes32,
    /// Transactions in the block
    pub transactions: List<Transaction, 1048576>,
    /// An empty list. This is unused and only exists for L1 compatibility.
    pub withdrawals: List<Withdrawal, 16>,
    /// The total gas used by the blob in the block
    pub blob_gas_used: u64,
    /// The excess gas used by the blob in the block
    pub excess_blob_gas: u64,
}

impl From<ExecutionPayloadV3SSZ> for L2ExecutionPayload {
    fn from(value: ExecutionPayloadV3SSZ) -> Self {
        Self {
            parent_hash: convert_hash(value.parent_hash),
            fee_recipient: convert_address(value.fee_recipient),
            state_root: convert_hash(value.state_root),
            receipts_root: convert_hash(value.receipts_root),
            logs_bloom: convert_bloom(value.logs_bloom),
            prev_randao: convert_hash(value.prev_randao),
            block_number: value.block_number,
            gas_limit: value.gas_limit.into(),
            gas_used: value.gas_used.into(),
            timestamp: value.timestamp,
            extra_data: convert_byte_list(value.extra_data),
            base_fee_per_gas: convert_uint(value.base_fee_per_gas),
            block_hash: convert_hash(value.block_hash),
            transactions: convert_tx_list(value.transactions),
            deserialized_transactions: Vec::default(),
            withdrawals: Some(Vec::new()),
            blob_gas_used: Some(value.blob_gas_used.into()),
            excess_blob_gas: Some(value.excess_blob_gas.into()),
        }
    }
}

/// Converts an [ssz_rs::Vector] of bytes into [alloy::primitives::Bloom]
fn convert_bloom(vector: Vector<u8, 256>) -> alloy::primitives::Bloom {
    let mut bloom = [0u8; 256];
    bloom.copy_from_slice(vector.as_ref());
    alloy::primitives::Bloom::from(bloom)
}

/// Converts [Bytes32] into [alloy::primitives::B256]
fn convert_hash(bytes: Bytes32) -> alloy::primitives::B256 {
    alloy::primitives::B256::from_slice(bytes.as_slice())
}

/// Converts [VecAddress] into [alloy::primitives::Address]
fn convert_address(address: VecAddress) -> alloy::primitives::Address {
    alloy::primitives::Address::from_slice(address.as_slice())
}

/// Converts an [ssz_rs::List] of bytes into [alloy::primitives::Bytes]
fn convert_byte_list<const N: usize>(list: List<u8, N>) -> alloy::primitives::Bytes {
    alloy::primitives::Bytes::from(list.to_vec())
}

/// Converts a [U256] into [u128]
fn convert_uint(value: U256) -> Option<u128> {
    let bytes = value.to_bytes_le();
    let bytes: [u8; 16] = bytes.try_into().ok()?;
    Some(u128::from_le_bytes(bytes))
}

/// Converts [ssz_rs::List] of [Transaction] into a vector of [alloy::primitives::Bytes]
fn convert_tx_list(value: List<Transaction, 1048576>) -> Vec<alloy::primitives::Bytes> {
    value.iter().map(|tx| alloy::primitives::Bytes::from(tx.to_vec())).collect()
}
