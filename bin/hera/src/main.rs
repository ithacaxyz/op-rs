//! Hera OP Stack Rollup node

#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/paradigmxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use clap::{Parser, Subcommand};
use eyre::Result;

mod network;
mod node;

/// The Hera CLI Arguments.
#[derive(Parser, Clone, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct HeraArgs {
    /// Global arguments for the Hera CLI.
    #[clap(flatten)]
    pub global: GlobalArgs,
    /// The subcommand to run.
    #[clap(subcommand)]
    pub subcommand: HeraSubcommand,
}

/// Global arguments for the Hera CLI.
#[derive(Parser, Clone, Debug)]
pub(crate) struct GlobalArgs {
    /// The L2 chain ID to use.
    #[clap(long, short = 'c', default_value = "10", help = "The L2 chain ID to use")]
    pub l2_chain_id: u64,
    /// A port to serve prometheus metrics on.
    #[clap(
        long,
        short = 'm',
        default_value = "9090",
        help = "The port to serve prometheus metrics on"
    )]
    pub metrics_port: u16,
}

/// Subcommands for the CLI.
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum HeraSubcommand {
    /// Run the standalone Hera node.
    Node(node::NodeCommand),
    /// Networking utility commands.
    Network(network::NetworkCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = HeraArgs::parse();
    rollup::init_telemetry_stack(args.global.metrics_port)?;
    tracing::info!("Hera OP Stack Rollup node");
    match args.subcommand {
        HeraSubcommand::Node(node) => node.run(&args.global).await?,
        HeraSubcommand::Network(network) => network.run(&args.global).await?,
    }
    Ok(())
}
