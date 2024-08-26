//! Contains a builder for the discovery service.

use crate::{
    discovery::driver::DiscoveryDriver,
    types::{address::NetworkAddress, enr::OpStackEnr},
};
use discv5::{
    enr::{CombinedKey, Enr},
    ConfigBuilder, Discv5, ListenConfig,
};
use eyre::Result;

/// Discovery service builder.
#[derive(Debug, Default, Clone)]
pub struct DiscoveryBuilder {
    /// The discovery service address.
    address: Option<NetworkAddress>,
    /// The chain ID of the network.
    chain_id: Option<u64>,
}

impl DiscoveryBuilder {
    /// Creates a new discovery builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the discovery service address.
    pub fn with_address(mut self, address: NetworkAddress) -> Self {
        self.address = Some(address);
        self
    }

    /// Sets the chain ID of the network.
    pub fn with_chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = Some(chain_id);
        self
    }

    /// Builds a [DiscoveryDriver].
    pub fn build(self) -> Result<DiscoveryDriver> {
        let addr = self.address.ok_or_else(|| eyre::eyre!("address not set"))?;
        let chain_id = self.chain_id.ok_or_else(|| eyre::eyre!("chain ID not set"))?;
        let opstack = OpStackEnr::new(chain_id, 0);
        let opstack_data: Vec<u8> = opstack.into();

        let key = CombinedKey::generate_secp256k1();
        let enr = Enr::builder().add_value_rlp("opstack", opstack_data.into()).build(&key)?;
        let listen_config = ListenConfig::from_ip(addr.ip.into(), addr.port);
        let config = ConfigBuilder::new(listen_config).build();

        let disc = Discv5::new(enr, key, config)
            .map_err(|_| eyre::eyre!("could not create disc service"))?;

        Ok(DiscoveryDriver::new(disc, chain_id))
    }
}
