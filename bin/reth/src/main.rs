#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/paradigmxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use std::sync::Arc;

use clap::Parser;
use eyre::{bail, Result};
use hera_exex::{HeraArgsExt, HeraExEx, HERA_EXEX_ID};
use reth::cli::Cli;
use reth_node_ethereum::EthereumNode;
use superchain_registry::ROLLUP_CONFIGS;

/// The Reth CLI arguments with optional Hera Execution Extension support.
#[derive(Debug, Clone, Parser)]
pub(crate) struct RethArgsExt {
    /// Whether to install the Hera Execution Extension.
    ///
    /// Additional Hera-specific flags will be parsed if this flag is set.
    #[clap(long, default_value_t = false)]
    pub hera: bool,
    /// The Hera Execution Extension configuration.
    ///
    /// This is only used if the `hera` flag is set.
    #[clap(flatten)]
    pub hera_config: Option<HeraArgsExt>,
}

fn main() -> Result<()> {
    Cli::<RethArgsExt>::parse().run(|builder, args| async move {
        if args.hera {
            // Spawn Reth with the Hera Execution Extension
            let Some(hera_args) = args.hera_config else {
                bail!("Hera Execution Extension configuration is required when the `hera` flag is set");
            };

            let Some(cfg) = ROLLUP_CONFIGS.get(&hera_args.l2_chain_id).cloned().map(Arc::new) else {
                bail!("Rollup configuration not found for L2 chain ID: {}", hera_args.l2_chain_id);
            };

            let node = EthereumNode::default();
            let hera = move |ctx| async { Ok(HeraExEx::new(ctx, hera_args, cfg).await.start()) };
            let handle = builder.node(node).install_exex(HERA_EXEX_ID, hera).launch().await?;
            handle.wait_for_node_exit().await
        } else {
            // Spawn Reth without the Hera Execution Extension
            let node = EthereumNode::default();
            let handle = builder.node(node).launch().await?;
            handle.wait_for_node_exit().await
        }
    })
}
