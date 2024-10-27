//! Global arguments for the Hera CLI.

use clap::{ArgAction, Parser};

/// Global arguments for the Hera CLI.
#[derive(Parser, Clone, Debug)]
pub(crate) struct GlobalArgs {
    /// Verbosity level (0-2)
    #[arg(long, short, action = ArgAction::Count)]
    pub v: u8,
    /// The L2 chain ID to use.
    #[clap(long, short = 'c', default_value = "10")]
    pub l2_chain_id: u64,
    /// The port to serve prometheus metrics on.
    #[clap(long, short = 'm', default_value = "9090")]
    pub metrics_port: u16,
}
