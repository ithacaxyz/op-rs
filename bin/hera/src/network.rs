//! Networking subcommand for Hera.

use crate::globals::GlobalArgs;
use clap::Args;
use eyre::Result;
use op_net::{discovery::builder::DiscoveryBuilder, driver::NetworkDriver};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use superchain_registry::ROLLUP_CONFIGS;

/// The Hera network subcommand.
#[derive(Debug, Clone, Args)]
#[non_exhaustive]
pub struct NetworkCommand {
    /// Run the peer discovery service.
    #[clap(long, short = 'p', help = "Runs only peer discovery")]
    pub only_disc: bool,
    /// Port to listen for gossip on.
    #[clap(long, short = 'l', default_value = "9099", help = "Port to listen for gossip on")]
    pub gossip_port: u16,
}

impl NetworkCommand {
    /// Run the network subcommand.
    pub async fn run(self, args: &GlobalArgs) -> Result<()> {
        if self.only_disc {
            self.run_discovery(args).await
        } else {
            self.run_network(args)
        }
    }

    /// Runs the full network.
    pub fn run_network(&self, args: &GlobalArgs) -> Result<()> {
        let signer = ROLLUP_CONFIGS
            .get(&args.l2_chain_id)
            .ok_or(eyre::eyre!("No rollup config found for chain ID"))?
            .genesis
            .system_config
            .as_ref()
            .ok_or(eyre::eyre!("No system config found for chain ID"))?
            .batcher_address;
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), self.gossip_port);
        let mut driver = NetworkDriver::builder()
            .with_chain_id(args.l2_chain_id)
            .with_unsafe_block_signer(signer)
            .with_gossip_addr(socket)
            .build()?;
        let recv =
            driver.take_unsafe_block_recv().ok_or(eyre::eyre!("No unsafe block receiver"))?;
        driver.start()?;
        loop {
            match recv.recv() {
                Ok(block) => {
                    tracing::info!("Received unsafe block: {:?}", block);
                }
                Err(e) => {
                    tracing::warn!("Failed to receive unsafe block: {:?}", e);
                }
            }
        }
    }

    /// Runs only the discovery service.
    pub async fn run_discovery(&self, args: &GlobalArgs) -> Result<()> {
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), self.gossip_port);
        let mut discovery_builder =
            DiscoveryBuilder::new().with_address(socket).with_chain_id(args.l2_chain_id);
        let discovery = discovery_builder.build()?;
        let mut peer_recv = discovery.start()?;
        loop {
            match peer_recv.recv().await {
                Some(peer) => {
                    tracing::info!("Received peer: {:?}", peer);
                }
                None => {
                    tracing::warn!("Failed to receive peer");
                }
            }
        }
    }
}
