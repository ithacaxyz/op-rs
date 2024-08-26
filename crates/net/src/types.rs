//! Types for the P2P network.

use alloy::primitives::{keccak256, Signature, B256};
use discv5::enr::{CombinedKey, Enr};
use eyre::Result;
use kona_primitives::L2ExecutionPayload;
use libp2p::{multiaddr::Protocol, Multiaddr};
use ssz_rs::prelude::*;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
};

use crate::ssz::{ExecutionPayloadV1SSZ, ExecutionPayloadV2SSZ, ExecutionPayloadV3SSZ};

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

/// Represents the Keccak256 hash of the block
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

/// An [Ipv4Addr] and port.
#[derive(Debug, Clone, Copy)]
pub struct NetworkAddress {
    /// An [Ipv4Addr]
    pub ip: Ipv4Addr,
    /// A port
    pub port: u16,
}

/// A wrapper around a peer's Network Address.
#[derive(Debug)]
pub struct Peer {
    /// The peer's [Ipv4Addr] and port
    pub addr: NetworkAddress,
}

impl TryFrom<&Enr<CombinedKey>> for NetworkAddress {
    type Error = eyre::Report;

    /// Convert an [Enr] to a Network Address.
    fn try_from(value: &Enr<CombinedKey>) -> Result<Self> {
        let ip = value.ip4().ok_or(eyre::eyre!("missing ip"))?;
        let port = value.tcp4().ok_or(eyre::eyre!("missing port"))?;

        Ok(Self { ip, port })
    }
}

impl From<NetworkAddress> for Multiaddr {
    /// Converts a Network Address to a [Multiaddr]
    fn from(value: NetworkAddress) -> Self {
        let mut multiaddr = Multiaddr::empty();
        multiaddr.push(Protocol::Ip4(value.ip));
        multiaddr.push(Protocol::Tcp(value.port));

        multiaddr
    }
}

impl From<NetworkAddress> for SocketAddr {
    /// Converts a Network Address to a [SocketAddr].
    fn from(value: NetworkAddress) -> Self {
        SocketAddr::new(IpAddr::V4(value.ip), value.port)
    }
}

impl TryFrom<SocketAddr> for NetworkAddress {
    type Error = eyre::Report;

    /// Converts a [SocketAddr] to a Network Address.
    fn try_from(value: SocketAddr) -> Result<Self> {
        let ip = match value.ip() {
            IpAddr::V4(ip) => ip,
            IpAddr::V6(_) => eyre::bail!("ipv6 not supported"),
        };

        Ok(Self { ip, port: value.port() })
    }
}

impl TryFrom<&Enr<CombinedKey>> for Peer {
    type Error = eyre::Report;

    /// Converts an [Enr] to a Peer
    fn try_from(value: &Enr<CombinedKey>) -> Result<Self> {
        let addr = NetworkAddress::try_from(value)?;
        Ok(Peer { addr })
    }
}

impl From<Peer> for Multiaddr {
    /// Converts a Peer to a [Multiaddr]
    fn from(value: Peer) -> Self {
        value.addr.into()
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
}
