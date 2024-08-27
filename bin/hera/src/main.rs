//! Hera OP Stack Rollup node

#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/paradigmxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use alloy::primitives::address;
use clap::{ArgAction, Parser};
use eyre::Result;
use op_net::driver::NetworkDriver;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tracing::Level;

/// The default L2 chain ID to use. This corresponds to OP Mainnet.
pub const DEFAULT_L2_CHAIN_ID: u64 = 10;

/// The Hera CLI Arguments.
#[derive(Debug, Clone, Parser)]
pub struct HeraArgs {
    /// Verbosity level (0-4)
    #[arg(long, short, help = "Verbosity level (0-4)", action = ArgAction::Count)]
    pub v: u8,
    /// Chain ID of the L2 network
    #[clap(long = "hera.l2-chain-id", default_value_t = DEFAULT_L2_CHAIN_ID)]
    pub l2_chain_id: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = HeraArgs::parse();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(match args.v {
            0 => Level::ERROR,
            1 => Level::WARN,
            2 => Level::INFO,
            3 => Level::DEBUG,
            _ => Level::TRACE,
        })
        .finish();
    tracing::subscriber::set_global_default(subscriber).map_err(|e| eyre::eyre!(e))?;

    let signer = address!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9099);
    let mut driver = NetworkDriver::builder()
        .with_chain_id(args.l2_chain_id)
        .with_unsafe_block_signer(signer)
        .with_socket(socket)
        .build()
        .expect("Failed to builder network driver");

    // Call `.start()` on the driver.
    let recv = driver.unsafe_block_recv.take().ok_or(eyre::eyre!("No unsafe block receiver"))?;
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
