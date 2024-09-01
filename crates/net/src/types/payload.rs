//! Execution Payload Types

use alloy::primitives::{keccak256, B256};
use ssz_rs::prelude::*;

/// Represents the Keccak256 hash of the block
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "arbitrary"), derive(arbitrary::Arbitrary))]
pub struct PayloadHash(B256);

impl From<&[u8]> for PayloadHash {
    /// Returns the Keccak256 hash of a sequence of bytes
    fn from(value: &[u8]) -> Self {
        Self(keccak256(value))
    }
}

impl PayloadHash {
    /// The expected message that should be signed by the unsafe block signer.
    pub fn signature_message(&self, chain_id: u64) -> B256 {
        let domain = B256::ZERO.as_slice();
        let chain_id = B256::left_padding_from(&chain_id.to_be_bytes()[..]);
        let payload_hash = self.0.as_slice();
        keccak256([domain, chain_id.as_slice(), payload_hash].concat())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::b256;

    #[test]
    fn test_signature_message() {
        let inner = b256!("9999999999999999999999999999999999999999999999999999999999999999");
        let hash = PayloadHash::from(inner.as_slice());
        let chain_id = 10;
        let expected = b256!("44a0e2b1aba1aae1771eddae1dcd2ad18a8cdac8891517153f03253e49d3f206");
        assert_eq!(hash.signature_message(chain_id), expected);
    }

    #[test]
    fn test_inner_payload_hash() {
        arbtest::arbtest(|u| {
            let inner = B256::from(u.arbitrary::<[u8; 32]>()?);
            let hash = PayloadHash::from(inner.as_slice());
            assert_eq!(hash.0, keccak256(inner.as_slice()));
            Ok(())
        });
    }
}
