use clap::Parser;
use eyre::Result;
use reth::cli::Cli;
use reth_node_ethereum::EthereumNode;

mod kona;
use kona::KonaExEx;

mod cli;
use cli::KonaArgsExt;

/// The identifier of the Kona Execution Extension.
const EXEX_ID: &str = "kona";

fn main() -> Result<()> {
    Cli::<KonaArgsExt>::parse().run(|builder, args| async move {
        let node = EthereumNode::default();
        let kona = move |ctx| async { Ok(KonaExEx::new(ctx, args).await.start()) };
        let handle = builder.node(node).install_exex(EXEX_ID, kona).launch().await?;
        handle.wait_for_node_exit().await
    })
}
