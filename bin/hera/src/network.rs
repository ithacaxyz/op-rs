//! Networking subcommand for Hera.

use crate::GlobalArgs;
use clap::Args;
use eyre::Result;
use op_net::driver::NetworkDriver;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use superchain_registry::ROLLUP_CONFIGS;

/// The Hera network subcommand.
#[derive(Debug, Clone, Args)]
#[non_exhaustive]
pub struct NetworkCommand {
    /// Run peer discovery.
    #[clap(long, short = 'p', help = "Runs peer discovery")]
    pub peer: bool,
    /// Run the gossip driver.
    #[clap(long, short = 'g', help = "Runs the unsafe block gossipping service")]
    pub gossip: bool,
}

impl NetworkCommand {
    /// Run the network subcommand.
    pub async fn run(&self, args: &GlobalArgs) -> Result<()> {
        if self.peer {
            println!("Running peer discovery");
        }
        if self.gossip {
            println!("Running gossip driver");
        }
        let signer = ROLLUP_CONFIGS
            .get(&args.l2_chain_id)
            .ok_or(eyre::eyre!("No rollup config found for chain ID"))?
            .genesis
            .system_config
            .as_ref()
            .ok_or(eyre::eyre!("No system config found for chain ID"))?
            .batcher_address;
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9099);
        let mut driver = NetworkDriver::builder()
            .with_chain_id(args.l2_chain_id)
            .with_unsafe_block_signer(signer)
            .with_gossip_addr(socket)
            .build()
            .expect("Failed to builder network driver");

        // Call `.start()` on the driver.
        let recv =
            driver.take_unsafe_block_recv().ok_or(eyre::eyre!("No unsafe block receiver"))?;
        driver.start().expect("Failed to start network driver");

        tracing::info!("NetworkDriver started successfully.");

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
}
