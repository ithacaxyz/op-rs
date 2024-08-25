//! Runner for OP-RS execution extensions

#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/paradigmxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use std::{fs::File, sync::Arc};

use clap::Parser;
use eyre::{bail, Context, Result};
use reth::cli::Cli;
use reth_node_ethereum::EthereumNode;
use serde_json::from_reader;
use superchain_registry::ROLLUP_CONFIGS;
use tracing::{debug, info, warn};

use rollup::{Driver, HeraArgsExt, HERA_EXEX_ID};

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
            info!("Running Reth with the Hera Execution Extension");
            let Some(hera_args) = args.hera_config else {
                bail!("Hera Execution Extension configuration is required when the `hera` flag is set");
            };

            let cfg = match &hera_args.l2_config_file {
                Some(path) => {
                    info!("Loading l2 config from file: {:?}", path);
                    let file = File::open(path).wrap_err("Failed to open l2 config file")?;
                    Arc::new(from_reader(file).wrap_err("Failed to read l2 config file")?)
                }
                None => {
                    debug!("Loading l2 config from superchain registry");
                    let Some(cfg) = ROLLUP_CONFIGS.get(&hera_args.l2_chain_id).cloned() else {
                        bail!("Failed to find l2 config for chain ID {}", hera_args.l2_chain_id);
                    };
                    Arc::new(cfg)
                }
            };

            let node = EthereumNode::default();
            let hera = move |ctx| async { Ok(Driver::new(ctx, hera_args, cfg).await.start()) };
            let handle = builder.node(node).install_exex(HERA_EXEX_ID, hera).launch().await?;
            handle.wait_for_node_exit().await
        } else {
            warn!("Running Reth without the Hera Execution Extension");
            let node = EthereumNode::default();
            let handle = builder.node(node).launch().await?;
            handle.wait_for_node_exit().await
        }
    })
}
