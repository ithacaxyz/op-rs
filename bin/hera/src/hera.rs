//! Module for the Hera CLI and its subcommands.

use clap::{Args, Parser, Subcommand};
use eyre::{bail, Result};
use reth::cli::Cli;
use reth_exex::ExExContext;
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;
use std::{path::PathBuf, sync::Arc};
use superchain_registry::{RollupConfig, ROLLUP_CONFIGS};
use url::Url;

/// The top-level Hera CLI Command
#[derive(Debug, Parser)]
#[command(author, about = "Hera", long_about = None)]
pub struct HeraCli {
    /// Hera's subcommands
    #[command(subcommand)]
    pub subcmd: HeraSubCmd,
}

impl HeraCli {
    /// Runs the Hera CLI
    pub fn run(self) -> Result<()> {
        match self.subcmd {
            HeraSubCmd::ExEx(cli) => cli.run(|builder, args| async move {
                let Some(cfg) = ROLLUP_CONFIGS.get(&args.l2_chain_id).cloned() else {
                    bail!("Rollup configuration not found for L2 chain ID: {}", args.l2_chain_id);
                };
                let node = EthereumNode::default();
                let kona =
                    move |ctx| async { Ok(HeraExEx::new(ctx, args, Arc::new(cfg)).await.start()) };
                let handle = builder.node(node).install_exex(crate::EXEX_ID, kona).launch().await?;
                handle.wait_for_node_exit().await
            }),
            HeraSubCmd::Bin => unimplemented!(),
        }
    }
}

/// The Hera subcommands
#[derive(Debug, Subcommand)]
pub enum HeraSubCmd {
    /// The Execution Extension
    #[clap(name = "exex")]
    ExEx(Cli<HeraArgsExt>),
    /// A standalone rollup node binary.
    #[clap(name = "bin")]
    Bin,
}

/// The default L2 chain ID to use. This corresponds to OP Mainnet.
pub const DEFAULT_L2_CHAIN_ID: u64 = 10;

/// The default L2 RPC URL to use.
pub const DEFAULT_L2_RPC_URL: &str = "https://optimism.llamarpc.com/";

/// The default L1 Beacon Client RPC URL to use.
pub const DEFAULT_L1_BEACON_CLIENT_URL: &str = "http://localhost:5052/";

/// The Hera Execution Extension CLI Arguments.
#[derive(Debug, Clone, Args)]
pub(crate) struct HeraArgsExt {
    /// Chain ID of the L2 network
    #[clap(long = "kona.l2-chain-id", default_value_t = DEFAULT_L2_CHAIN_ID)]
    pub l2_chain_id: u64,

    /// RPC URL of an L2 execution client
    #[clap(long = "kona.l2-rpc-url", default_value = DEFAULT_L2_RPC_URL)]
    pub l2_rpc_url: Url,

    /// URL of an L1 beacon client to fetch blobs
    #[clap(long = "kona.l1-beacon-client-url", default_value = DEFAULT_L1_BEACON_CLIENT_URL)]
    pub l1_beacon_client_url: Url,

    /// URL of the blob archiver to fetch blobs that are expired on
    /// the beacon client but still needed for processing.
    ///
    /// Blob archivers need to implement the `blob_sidecars` API:
    /// <https://ethereum.github.io/beacon-APIs/#/Beacon/getBlobSidecars>
    #[clap(long = "kona.l1-blob-archiver-url")]
    pub l1_blob_archiver_url: Option<Url>,

    /// The payload validation mode to use.
    ///
    /// - Trusted: rely on a trusted synced L2 execution client. Validation happens by fetching the
    ///   same block and comparing the results.
    /// - Engine API: use a local or remote engine API of an L2 execution client. Validation
    ///   happens by sending the `new_payload` to the API and expecting a VALID response.
    #[clap(
        long = "kona.validation-mode",
        default_value = "trusted",
        requires_ifs([("engine-api", "l2-engine-api-url"), ("engine-api", "l2-engine-jwt-secret")]),
    )]
    pub validation_mode: ValidationMode,

    /// If the mode is "engine api", we also need an URL for the engine API endpoint of
    /// the execution client to validate the payload.
    #[clap(long = "kona.l2-engine-api-url")]
    pub l2_engine_api_url: Option<Url>,

    /// If the mode is "engine api", we also need a JWT secret for the auth-rpc.
    /// This MUST be a valid path to a file containing the hex-encoded JWT secret.
    #[clap(long = "kona.l2-engine-jwt-secret")]
    pub l2_engine_jwt_secret: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) enum ValidationMode {
    Trusted,
    EngineApi,
}

impl std::str::FromStr for ValidationMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "trusted" => Ok(ValidationMode::Trusted),
            "engine-api" => Ok(ValidationMode::EngineApi),
            _ => Err(format!("Invalid validation mode: {}", s)),
        }
    }
}

/// The Hera Execution Extension.
#[derive(Debug)]
#[allow(unused)]
pub(crate) struct HeraExEx<Node: FullNodeComponents> {
    /// The rollup configuration
    cfg: Arc<RollupConfig>,
    /// The context of the Execution Extension
    ctx: ExExContext<Node>,
}

#[allow(unused)]
impl<Node: FullNodeComponents> HeraExEx<Node> {
    /// Creates a new instance of the Hera Execution Extension.
    pub async fn new(ctx: ExExContext<Node>, args: HeraArgsExt, cfg: Arc<RollupConfig>) -> Self {
        Self { ctx, cfg }
    }

    /// Starts the Execution Extension loop.
    pub async fn start(mut self) -> Result<()> {
        unimplemented!()
    }
}