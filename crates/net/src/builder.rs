//! Network Builder Module.

use alloy::primitives::Address;
use eyre::Result;
use std::net::SocketAddr;
use tokio::sync::watch::channel;

use libp2p::{
    gossipsub::ConfigBuilder, noise::Config as NoiseConfig, tcp::Config as TcpConfig,
    yamux::Config as YamuxConfig, Multiaddr, SwarmBuilder,
};
use libp2p_identity::Keypair;

use crate::{
    discovery::builder::DiscoveryBuilder,
    driver::NetworkDriver,
    gossip::{behaviour::Behaviour, config, driver::GossipDriver, handler::BlockHandler},
    types::address::NetworkAddress,
};

/// Constructs a [NetworkDriver] for Optimism's consensus-layer.
#[derive(Default)]
pub struct NetworkDriverBuilder {
    /// The chain ID of the network.
    chain_id: Option<u64>,
    /// The unsafe block signer.
    unsafe_block_signer: Option<Address>,
    /// The socket address that the service is listening on.
    socket: Option<SocketAddr>,
    /// The [ConfigBuilder] constructs the config for `gossipsub`.
    inner: Option<ConfigBuilder>,
    /// The [Keypair] for the node.
    keypair: Option<Keypair>,
    /// The [TcpConfig] for the swarm.
    tcp_config: Option<TcpConfig>,
    /// The [NoiseConfig] for the swarm.
    noise_config: Option<NoiseConfig>,
    /// The [YamuxConfig] for the swarm.
    yamux_config: Option<YamuxConfig>,
}

impl NetworkDriverBuilder {
    /// Creates a new [NetworkDriverBuilder].
    pub fn new() -> Self {
        Self::default()
    }

    /// Specifies the chain ID of the network.
    pub fn with_chain_id(&mut self, chain_id: u64) -> &mut Self {
        self.chain_id = Some(chain_id);
        self
    }

    /// Specifies the unsafe block signer.
    pub fn with_unsafe_block_signer(&mut self, unsafe_block_signer: Address) -> &mut Self {
        self.unsafe_block_signer = Some(unsafe_block_signer);
        self
    }

    /// Specifies the socket address that the service is listening on.
    pub fn with_socket(&mut self, socket: SocketAddr) -> &mut Self {
        self.socket = Some(socket);
        self
    }

    /// Specifies the keypair for the node.
    pub fn with_keypair(&mut self, keypair: Keypair) -> &mut Self {
        self.keypair = Some(keypair);
        self
    }

    /// Specifies the [TcpConfig] for the swarm.
    pub fn with_tcp_config(&mut self, tcp_config: TcpConfig) -> &mut Self {
        self.tcp_config = Some(tcp_config);
        self
    }

    /// Specifies the [NoiseConfig] for the swarm.
    pub fn with_noise_config(&mut self, noise_config: NoiseConfig) -> &mut Self {
        self.noise_config = Some(noise_config);
        self
    }

    /// Specifies the [YamuxConfig] for the swarm.
    pub fn with_yamux_config(&mut self, yamux_config: YamuxConfig) -> &mut Self {
        self.yamux_config = Some(yamux_config);
        self
    }

    // TODO: extend builder with [ConfigBuilder] methods.

    /// Specifies the [ConfigBuilder] for the `gossipsub` configuration.
    pub fn with_gossip_config(&mut self, inner: ConfigBuilder) -> &mut Self {
        self.inner = Some(inner);
        self
    }

    /// Builds the [NetworkDriver].
    pub fn build(&mut self) -> Result<NetworkDriver> {
        // Build the config for gossipsub.
        let config = self.inner.take().unwrap_or(config::default_config_builder()).build()?;
        let unsafe_block_signer =
            self.unsafe_block_signer.ok_or_else(|| eyre::eyre!("unsafe block signer not set"))?;
        let chain_id = self.chain_id.ok_or_else(|| eyre::eyre!("chain ID not set"))?;

        // Create the block handler.
        let (unsafe_block_signer_sender, unsafe_block_signer_recv) = channel(unsafe_block_signer);
        let (handler, unsafe_block_recv) = BlockHandler::new(chain_id, unsafe_block_signer_recv);

        // Construct the gossipsub behaviour.
        let behaviour = Behaviour::new(config, &[Box::new(handler.clone())])?;

        // Build the swarm.
        let noise_config = self.noise_config.take();
        let keypair = self.keypair.take().unwrap_or(Keypair::generate_secp256k1());
        let swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                self.tcp_config.take().unwrap_or_default(),
                |i: &Keypair| match noise_config {
                    Some(cfg) => core::result::Result::Ok(cfg),
                    None => NoiseConfig::new(i),
                },
                || self.yamux_config.take().unwrap_or_default(),
            )?
            .with_behaviour(|_| behaviour)?
            .build();
        let addr = self.socket.take().ok_or_else(|| eyre::eyre!("socket address not set"))?;
        let addr = NetworkAddress::try_from(addr)?;
        let swarm_addr = Multiaddr::from(addr);
        let gossip = GossipDriver::new(swarm, swarm_addr, handler);

        // Build the discovery service
        let discovery =
            DiscoveryBuilder::new().with_address(addr).with_chain_id(chain_id).build()?;

        Ok(NetworkDriver { unsafe_block_recv, unsafe_block_signer_sender, gossip, discovery })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::gossipsub::IdentTopic;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn test_build_missing_unsafe_block_signer() {
        let mut builder = NetworkDriverBuilder::new();
        let Err(err) = builder.build() else {
            panic!("expected error when building NetworkDriver without unsafe block signer");
        };
        assert_eq!(err.to_string(), "unsafe block signer not set");
    }

    #[test]
    fn test_build_missing_chain_id() {
        let mut builder = NetworkDriverBuilder::new();
        let Err(err) = builder.with_unsafe_block_signer(Address::random()).build() else {
            panic!("expected error when building NetworkDriver without chain id");
        };
        assert_eq!(err.to_string(), "chain ID not set");
    }

    #[test]
    fn test_build_missing_socket() {
        let mut builder = NetworkDriverBuilder::new();
        let Err(err) = builder.with_unsafe_block_signer(Address::random()).with_chain_id(1).build()
        else {
            panic!("expected error when building NetworkDriver without socket");
        };
        assert_eq!(err.to_string(), "socket address not set");
    }

    #[test]
    fn test_build_default_network_driver() {
        let id = 10;
        let signer = Address::random();
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9099);
        let driver = NetworkDriverBuilder::new()
            .with_unsafe_block_signer(signer)
            .with_chain_id(id)
            .with_socket(socket)
            .build()
            .unwrap();
        let signer_net_addr = NetworkAddress::try_from(socket).expect("network address");
        let signer_multiaddr = Multiaddr::from(signer_net_addr);

        // Driver Assertions
        assert_eq!(driver.gossip.addr, signer_multiaddr);
        assert_eq!(driver.discovery.chain_id, id);

        // Block Handler Assertions
        assert_eq!(driver.gossip.handler.chain_id, id);
        let v1 = IdentTopic::new(format!("/optimism/{}/0/blocks", id));
        assert_eq!(driver.gossip.handler.blocks_v1_topic.hash(), v1.hash());
        let v2 = IdentTopic::new(format!("/optimism/{}/1/blocks", id));
        assert_eq!(driver.gossip.handler.blocks_v2_topic.hash(), v2.hash());
        let v3 = IdentTopic::new(format!("/optimism/{}/2/blocks", id));
        assert_eq!(driver.gossip.handler.blocks_v3_topic.hash(), v3.hash());
    }
}
