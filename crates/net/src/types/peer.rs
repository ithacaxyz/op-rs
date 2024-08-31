//! Peer Types

#[cfg(any(test, feature = "arbitrary"))]
use arbitrary::{Arbitrary, Unstructured};
use discv5::enr::{CombinedKey, Enr};
use eyre::Result;
use libp2p::{multiaddr::Protocol, Multiaddr};
use std::net::{IpAddr, SocketAddr};

/// A wrapper around a peer's [SocketAddr].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Peer {
    /// The peer's [SocketAddr].
    pub socket: SocketAddr,
}

#[cfg(any(test, feature = "arbitrary"))]
impl<'a> Arbitrary<'a> for Peer {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        match u.arbitrary::<bool>()? {
            true => {
                let ipv6 = u.arbitrary::<[u8; 16]>()?;
                let port = u.arbitrary::<u16>()?;
                Ok(Peer { socket: SocketAddr::new(IpAddr::V6(ipv6.into()), port) })
            }
            false => {
                let ipv4 = u.arbitrary::<u8>()?;
                let port = u.arbitrary::<u16>()?;
                Ok(Peer { socket: SocketAddr::new(IpAddr::V4([ipv4; 4].into()), port) })
            }
        }
    }
}

impl TryFrom<&Enr<CombinedKey>> for Peer {
    type Error = eyre::Report;

    /// Converts an [Enr] to a Peer
    fn try_from(value: &Enr<CombinedKey>) -> Result<Self> {
        let ip = value.ip4().ok_or(eyre::eyre!("missing ip"))?;
        let port = value.tcp4().ok_or(eyre::eyre!("missing port"))?;
        let socket = SocketAddr::new(IpAddr::V4(ip), port);
        Ok(Peer { socket })
    }
}

impl From<Peer> for Multiaddr {
    /// Converts a Peer to a [Multiaddr]
    fn from(value: Peer) -> Self {
        let mut multiaddr = Multiaddr::empty();
        match value.socket.ip() {
            IpAddr::V4(ip) => multiaddr.push(Protocol::Ip4(ip)),
            IpAddr::V6(ip) => multiaddr.push(Protocol::Ip6(ip)),
        }
        multiaddr.push(Protocol::Tcp(value.socket.port()));
        multiaddr
    }
}

impl TryFrom<&Multiaddr> for Peer {
    type Error = eyre::Report;

    /// Converts a [Multiaddr] to a Peer
    fn try_from(value: &Multiaddr) -> Result<Self> {
        let mut ip = None;
        let mut port = None;
        for protocol in value.iter() {
            match protocol {
                Protocol::Ip4(ip4) => {
                    ip = Some(IpAddr::V4(ip4));
                }
                Protocol::Ip6(ip6) => {
                    ip = Some(IpAddr::V6(ip6));
                }
                Protocol::Tcp(tcp) => {
                    port = Some(tcp);
                }
                _ => {}
            }
        }
        let ip = ip.ok_or(eyre::eyre!("missing ip"))?;
        let port = port.ok_or(eyre::eyre!("missing port"))?;
        Ok(Peer { socket: SocketAddr::new(ip, port) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_to_multiaddr() {
        arbtest::arbtest(|u| {
            let peer = Peer::arbitrary(u)?;
            let multiaddr = Multiaddr::from(peer.clone());
            let peer2 =
                Peer::try_from(&multiaddr).map_err(|_| arbitrary::Error::IncorrectFormat)?;
            assert_eq!(peer, peer2);
            Ok(())
        });
    }

    #[test]
    fn test_peer_from_enr_without_ip() {
        let key = CombinedKey::generate_secp256k1();
        let enr = Enr::<CombinedKey>::builder().build(&key).unwrap();
        let err = Peer::try_from(&enr).unwrap_err();
        assert_eq!(err.to_string(), "missing ip");
    }

    #[test]
    fn test_peer_from_enr_without_port() {
        let key = CombinedKey::generate_secp256k1();
        let ip = std::net::Ipv4Addr::new(192, 168, 0, 1);
        let enr = Enr::<CombinedKey>::builder().ip4(ip).build(&key).unwrap();
        let err = Peer::try_from(&enr).unwrap_err();
        assert_eq!(err.to_string(), "missing port");
    }

    #[test]
    fn test_peer_from_enr_succeeds() {
        let key = CombinedKey::generate_secp256k1();
        let ip = std::net::Ipv4Addr::new(192, 168, 0, 1);
        let port = 30303;
        let enr = Enr::<CombinedKey>::builder().ip4(ip).tcp4(port).build(&key).unwrap();
        let peer = Peer::try_from(&enr).unwrap();
        assert_eq!(peer.socket, SocketAddr::new(IpAddr::V4(ip), port));
    }
}
