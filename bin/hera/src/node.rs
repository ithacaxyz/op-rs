//! Node subcommand for Hera.

use clap::Args;
use eyre::{bail, Result};
use rollup::{Driver, HeraArgsExt};
use tracing::info;

use crate::globals::GlobalArgs;

/// The Hera node subcommand.
#[derive(Debug, Clone, Args)]
#[non_exhaustive]
pub struct NodeCommand {
    /// The Hera Rollup node configuration.
    #[clap(flatten)]
    pub hera_config: HeraArgsExt,
}

impl NodeCommand {
    /// Run the node subcommand.
    pub async fn run(self, _args: &GlobalArgs) -> Result<()> {
        info!(
            "Running the Hera Node in Standalone mode. Attributes validation: {}",
            self.hera_config.validation_mode
        );

        let cfg = self.hera_config.get_l2_config()?;
        let driver = Driver::standalone(self.hera_config, cfg).await?;

        if let Err(e) = driver.start().await {
            bail!("[CRIT] Rollup driver failed: {:?}", e)
        }

        Ok(())
    }
}
