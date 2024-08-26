//! Contains the Optimism consensus-layer ENR Type.

use discv5::enr::{CombinedKey, Enr};
use eyre::Result;
use unsigned_varint::{decode, encode};

/// The ENR key literal string for the consensus layer.
pub const OP_CL_KEY: &str = "opstack";

/// The unique L2 network identifier
#[derive(Debug, Clone, Copy, Default)]
pub struct OpStackEnr {
    /// Chain ID
    pub chain_id: u64,
    /// The version. Always set to 0.
    pub version: u64,
}

impl OpStackEnr {
    /// Instantiates a new Op Stack Enr.
    pub fn new(chain_id: u64, version: u64) -> Self {
        Self { chain_id, version }
    }

    /// Returns `true` if a node [Enr] contains an `opstack` key and is on the same network.
    pub fn is_valid_node(node: &Enr<CombinedKey>, chain_id: u64) -> bool {
        node.get_raw_rlp(OP_CL_KEY)
            .map(|opstack| {
                OpStackEnr::try_from(opstack)
                    .map(|opstack| opstack.chain_id == chain_id && opstack.version == 0)
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }
}

impl TryFrom<&[u8]> for OpStackEnr {
    type Error = eyre::Report;

    /// Converts a slice of RLP encoded bytes to Op Stack Enr Data.
    fn try_from(value: &[u8]) -> Result<Self> {
        // TODO: rlp decode first?
        // let bytes = Vec::<u8>::decode(&mut value)?;
        let mut bytes = value;
        let (chain_id, rest) =
            decode::u64(bytes).map_err(|_| eyre::eyre!("could not decode chain id"))?;
        bytes = rest;
        let (version, _) =
            decode::u64(bytes).map_err(|_| eyre::eyre!("could not decode chain id"))?;

        Ok(Self { chain_id, version })
    }
}

impl From<OpStackEnr> for Vec<u8> {
    /// Converts Op Stack Enr data to a vector of bytes.
    fn from(value: OpStackEnr) -> Vec<u8> {
        let mut chain_id_buf = encode::u128_buffer();
        let chain_id_slice = encode::u128(value.chain_id as u128, &mut chain_id_buf);

        let mut version_buf = encode::u128_buffer();
        let version_slice = encode::u128(value.version as u128, &mut version_buf);

        let opstack = [chain_id_slice, version_slice].concat();

        alloy_rlp::encode(&opstack).to_vec()
    }
}
