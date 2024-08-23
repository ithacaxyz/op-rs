#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/paradigmxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use std::sync::Arc;

use clap::Parser;
use eyre::{bail, Result};
use reth::cli::Cli;
use reth_node_ethereum::EthereumNode;
use superchain_registry::ROLLUP_CONFIGS;

mod kona;
use kona::KonaExEx;

mod cli;
use cli::KonaArgsExt;

/// The identifier of the Kona Execution Extension.
const EXEX_ID: &str = "kona";

fn main() -> Result<()> {
    Cli::<KonaArgsExt>::parse().run(|builder, args| async move {
        let Some(cfg) = ROLLUP_CONFIGS.get(&args.l2_chain_id).cloned() else {
            bail!("Rollup configuration not found for L2 chain ID: {}", args.l2_chain_id);
        };

        let node = EthereumNode::default();
        let kona = move |ctx| async { Ok(KonaExEx::new(ctx, args, Arc::new(cfg)).await.start()) };
        let handle = builder.node(node).install_exex(EXEX_ID, kona).launch().await?;
        handle.wait_for_node_exit().await
    })
}
