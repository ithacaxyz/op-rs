//! Node subcommand for Hera.

use crate::GlobalArgs;
use clap::Args;
use eyre::Result;

/// The Hera node subcommand.
#[derive(Debug, Clone, Args)]
#[non_exhaustive]
pub struct NodeCommand {}

impl NodeCommand {
    /// Run the node subcommand.
    pub async fn run(&self, _args: &GlobalArgs) -> Result<()> {
        unimplemented!()
    }
}
