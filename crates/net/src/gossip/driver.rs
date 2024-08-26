//! Consensus-layer gossipsub driver for Optimism.

use crate::gossip::{behaviour::Behaviour, event::Event};
use eyre::Result;
use futures::stream::StreamExt;
use libp2p::{Multiaddr, Swarm};

/// A [libp2p::Swarm] instance with an associated address to listen on.
pub struct GossipDriver {
    /// The [libp2p::Swarm] instance.
    pub swarm: Swarm<Behaviour>,
    /// The address to listen on.
    pub addr: Multiaddr,
}

impl GossipDriver {
    /// Creates a new [SwarmWithAddr] instance.
    pub fn new(swarm: Swarm<Behaviour>, addr: Multiaddr) -> Self {
        Self { swarm, addr }
    }

    /// Listens on the address.
    pub fn listen(&mut self) -> Result<()> {
        self.swarm.listen_on(self.addr.clone()).map_err(|_| eyre::eyre!("swarm listen failed"))?;
        Ok(())
    }

    /// Returns a mutable reference to the Swarm's behaviour.
    pub fn behaviour_mut(&mut self) -> &mut Behaviour {
        self.swarm.behaviour_mut()
    }

    /// Attempts to select the next event from the Swarm.
    pub async fn select_next_some(&mut self) -> libp2p::swarm::SwarmEvent<Event> {
        self.swarm.select_next_some().await
    }

    /// Dials the given [Multiaddr].
    pub async fn dial(&mut self, peer: impl Into<Multiaddr>) -> Result<()> {
        let addr: Multiaddr = peer.into();
        self.swarm.dial(addr).map_err(|_| eyre::eyre!("dial failed"))?;
        Ok(())
    }
}
