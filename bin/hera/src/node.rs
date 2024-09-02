//! Node subcommand for Hera.

use clap::Args;
use eyre::{bail, Result};
use rollup::{Driver, HeraArgsExt, StandaloneContext};
use url::Url;

use crate::globals::GlobalArgs;

/// The Hera node subcommand.
#[derive(Debug, Clone, Args)]
#[non_exhaustive]
pub struct NodeCommand {
    /// The URL of the L1 RPC provider.
    #[clap(long, short = 'l', help = "The URL of the L1 RPC")]
    pub l1_rpc_url: Url,

    /// The Hera Rollup node configuration.
    #[clap(flatten)]
    pub hera_config: HeraArgsExt,
}

impl NodeCommand {
    /// Run the node subcommand.
    pub async fn run(&self, _args: &GlobalArgs) -> Result<()> {
        let ctx = StandaloneContext::new(self.l1_rpc_url.clone()).await?;
        let hera_config = self.hera_config.clone();
        let cfg = hera_config.get_l2_config()?;

        let driver = Driver::standalone(ctx, hera_config, cfg);

        if let Err(e) = driver.start().await {
            bail!("Critical: Rollup driver failed: {:?}", e)
        }

        Ok(())
    }
}
