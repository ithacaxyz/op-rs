//! Hera OP Stack Rollup node

#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/ithacaxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use clap::{Parser, Subcommand};
use eyre::Result;

mod globals;
mod network;
mod node;

/// The Hera CLI Arguments.
#[derive(Parser, Clone, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct HeraArgs {
    /// Global arguments for the Hera CLI.
    #[clap(flatten)]
    pub global: globals::GlobalArgs,
    /// The subcommand to run.
    #[clap(subcommand)]
    pub subcommand: HeraSubcommand,
}

/// Subcommands for the CLI.
#[derive(Debug, Clone, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum HeraSubcommand {
    /// Run the standalone Hera node.
    Node(node::NodeCommand),
    /// Networking utility commands.
    Network(network::NetworkCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse arguments.
    let args = HeraArgs::parse();

    // Initialize the telemetry stack.
    rollup::init_telemetry_stack(args.global.metrics_port)?;

    // Dispatch on subcommand.
    match args.subcommand {
        HeraSubcommand::Node(node) => node.run(&args.global).await,
        HeraSubcommand::Network(network) => network.run(&args.global).await,
    }
}
