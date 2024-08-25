//! Module for the Hera Execution Extension CLI arguments.

use std::path::PathBuf;

use clap::Args;
use url::Url;

/// The default L2 chain ID to use. This corresponds to OP Mainnet.
pub const DEFAULT_L2_CHAIN_ID: u64 = 10;

/// The default L2 RPC URL to use.
pub const DEFAULT_L2_RPC_URL: &str = "https://optimism.llamarpc.com/";

/// The default L1 Beacon Client RPC URL to use.
pub const DEFAULT_L1_BEACON_CLIENT_URL: &str = "http://localhost:5052/";

/// The Hera Execution Extension CLI Arguments.
#[derive(Debug, Clone, Args)]
pub struct HeraArgsExt {
    /// Chain ID of the L2 network
    #[clap(long = "hera.l2-chain-id", default_value_t = DEFAULT_L2_CHAIN_ID)]
    pub l2_chain_id: u64,

    /// RPC URL of an L2 execution client
    #[clap(long = "hera.l2-rpc-url", default_value = DEFAULT_L2_RPC_URL)]
    pub l2_rpc_url: Url,

    /// URL of an L1 beacon client to fetch blobs
    #[clap(long = "hera.l1-beacon-client-url", default_value = DEFAULT_L1_BEACON_CLIENT_URL)]
    pub l1_beacon_client_url: Url,

    /// URL of the blob archiver to fetch blobs that are expired on
    /// the beacon client but still needed for processing.
    ///
    /// Blob archivers need to implement the `blob_sidecars` API:
    /// <https://ethereum.github.io/beacon-APIs/#/Beacon/getBlobSidecars>
    #[clap(long = "hera.l1-blob-archiver-url")]
    pub l1_blob_archiver_url: Option<Url>,

    /// The payload validation mode to use.
    ///
    /// - Trusted: rely on a trusted synced L2 execution client. Validation happens by fetching the
    ///   same block and comparing the results.
    /// - Engine API: use a local or remote engine API of an L2 execution client. Validation
    ///   happens by sending the `new_payload` to the API and expecting a VALID response.
    #[clap(
        long = "hera.validation-mode",
        default_value = "trusted",
        requires_ifs([("engine-api", "l2-engine-api-url"), ("engine-api", "l2-engine-jwt-secret")]),
    )]
    pub validation_mode: ValidationMode,

    /// If the mode is "engine api", we also need an URL for the engine API endpoint of
    /// the execution client to validate the payload.
    #[clap(long = "hera.l2-engine-api-url")]
    pub l2_engine_api_url: Option<Url>,

    /// If the mode is "engine api", we also need a JWT secret for the auth-rpc.
    /// This MUST be a valid path to a file containing the hex-encoded JWT secret.
    #[clap(long = "hera.l2-engine-jwt-secret")]
    pub l2_engine_jwt_secret: Option<PathBuf>,
}

/// The payload validation mode.
///
/// Every newly derived payload needs to be validated against a local
/// execution of all transactions included inside it. This can be done
/// in two ways:
///
/// - Trusted: rely on a trusted synced L2 execution client. Validation happens by fetching the same
///   block and comparing the results.
/// - Engine API: use the authenticated engine API of an L2 execution client. Validation happens by
///   sending the `new_payload` to the API and expecting a VALID response. This method can also be
///   used to verify unsafe payloads from the sequencer.
#[derive(Debug, Clone)]
pub enum ValidationMode {
    /// Use a trusted synced L2 execution client.
    Trusted,
    /// Use the authenticated engine API of an L2 execution client.
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