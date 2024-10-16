use alloy::{
    network::AnyNetwork, primitives::Bytes, providers::RootProvider, rpc::client::RpcClient,
};
use alloy_transport_http::{
    hyper_util::{
        client::legacy::{connect::HttpConnector, Client},
        rt::TokioExecutor,
    },
    AuthLayer, AuthService, Http, HyperClient,
};
use eyre::Result;
use http_body_util::Full;
use op_alloy_provider::ext::engine::OpEngineApi;
use op_alloy_rpc_types_engine::OpAttributesWithParent;
use reth::rpc::types::engine::{ForkchoiceState, JwtSecret};
use tower::ServiceBuilder;
use tracing::warn;
use url::Url;

/// A hyper client with a JWT auth layer middleware.
type HyperAuthClient<B = Full<Bytes>> = HyperClient<B, AuthService<Client<HttpConnector, B>>>;

/// EngineApiValidator
///
/// Validates the [`OpAttributesWithParent`] by sending the attributes to an L2 engine API.
/// The engine API will return a `VALID` or `INVALID` response.
#[derive(Debug, Clone)]
pub struct Engine {
    provider: RootProvider<Http<HyperAuthClient>, AnyNetwork>,
}

impl Engine {
    /// Creates a new [`EngineApiValidator`] from the provided [Url] and [JwtSecret].
    pub fn new_http(url: Url, jwt: JwtSecret) -> Self {
        let hyper_client = Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>();

        let auth_layer = AuthLayer::new(jwt);
        let service = ServiceBuilder::new().layer(auth_layer).service(hyper_client);

        let layer_transport = HyperClient::with_service(service);
        let http_hyper = Http::with_client(layer_transport, url);
        let rpc_client = RpcClient::new(http_hyper, true);
        let provider = RootProvider::<_, AnyNetwork>::new(rpc_client);

        Self { provider }
    }

    async fn validate_payload_fcu(&self, attributes: &OpAttributesWithParent) -> Result<bool> {
        // TODO: use the correct values
        let fork_choice_state = ForkchoiceState {
            head_block_hash: attributes.parent.block_info.hash,
            finalized_block_hash: attributes.parent.block_info.hash,
            safe_block_hash: attributes.parent.block_info.hash,
        };

        let attributes = Some(attributes.attributes.clone());
        let fcu = self.provider.fork_choice_updated_v2(fork_choice_state, attributes).await?;

        if fcu.is_valid() {
            Ok(true)
        } else {
            warn!(status = %fcu.payload_status, "Engine API returned invalid fork choice update");
            Ok(false)
        }
    }
}
