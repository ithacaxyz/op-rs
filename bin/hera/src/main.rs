//! Hera OP Stack Rollup node

#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/paradigmxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use alloy::primitives::address;
use clap::Parser;
use eyre::Result;
use op_net::driver::NetworkDriver;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// The default L2 chain ID to use. This corresponds to OP Mainnet.
pub const DEFAULT_L2_CHAIN_ID: u64 = 10;

/// The Hera CLI Arguments.
#[derive(Debug, Clone, Parser)]
pub struct HeraArgs {
    /// Chain ID of the L2 network
    #[clap(long = "hera.l2-chain-id", default_value_t = DEFAULT_L2_CHAIN_ID)]
    pub l2_chain_id: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = HeraArgs::parse();
    rollup::init_telemetry_stack(8090)?;

    tracing::info!("Hera OP Stack Rollup node");

    let signer = address!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9099);
    let mut driver = NetworkDriver::builder()
        .with_chain_id(args.l2_chain_id)
        .with_unsafe_block_signer(signer)
        .with_gossip_addr(socket)
        .build()
        .expect("Failed to builder network driver");

    // Call `.start()` on the driver.
    let recv = driver.take_unsafe_block_recv().ok_or(eyre::eyre!("No unsafe block receiver"))?;
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
