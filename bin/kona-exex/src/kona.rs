use std::sync::Arc;

use eyre::Result;
use reth_exex::ExExContext;
use reth_node_api::FullNodeComponents;
use superchain_registry::RollupConfig;

use crate::cli::KonaArgsExt;

#[derive(Debug)]
pub(crate) struct KonaExEx<Node: FullNodeComponents> {
    /// The rollup configuration
    cfg: Arc<RollupConfig>,
    /// The context of the Execution Extension
    ctx: ExExContext<Node>,
}

impl<Node: FullNodeComponents> KonaExEx<Node> {
    /// Creates a new instance of the Kona Execution Extension.
    pub async fn new(ctx: ExExContext<Node>, args: KonaArgsExt) -> Self {
        unimplemented!()
    }

    /// Starts the Kona Execution Extension loop.
    pub async fn start(mut self) -> Result<()> {
        unimplemented!()
    }
}
