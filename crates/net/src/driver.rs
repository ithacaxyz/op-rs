//! Driver for network services.

use alloy::primitives::Address;
use eyre::Result;
use libp2p::swarm::SwarmEvent;
use tokio::{
    select,
    sync::watch::{Receiver, Sender},
};

use crate::{
    builder::NetworkDriverBuilder,
    discovery::driver::DiscoveryDriver,
    gossip::{
        driver::GossipDriver,
        event::Event,
        handler::{BlockHandler, Handler},
    },
    types::envelope::ExecutionPayloadEnvelope,
};

/// NetworkDriver
///
/// Contains the logic to run Optimism's consensus-layer networking stack.
/// There are two core services that are run by the driver:
/// - Block gossip through Gossipsub.
/// - Peer discovery with `discv5`.
pub struct NetworkDriver {
    /// Channel to receive unsafe blocks.
    pub unsafe_block_recv: Receiver<ExecutionPayloadEnvelope>,
    /// Channel to send unsafe signer updates.
    pub unsafe_block_signer_sender: Sender<Address>,
    /// Block handler.
    pub handler: BlockHandler,
    /// The swarm instance.
    pub swarm: GossipDriver,
    /// The discovery service driver.
    pub discovery: DiscoveryDriver,
}

impl NetworkDriver {
    /// Returns a new [NetworkDriverBuilder].
    pub fn builder() -> NetworkDriverBuilder {
        NetworkDriverBuilder::new()
    }

    /// Starts the Discv5 peer discovery & libp2p services
    /// and continually listens for new peers and messages to handle
    pub fn start(mut self) -> Result<()> {
        let mut peer_recv = self.discovery.start()?;
        self.swarm.listen()?;
        tokio::spawn(async move {
            loop {
                select! {
                    peer = peer_recv.recv() => {
                        if let Some(peer) = peer {
                            _ = self.swarm.dial(peer);
                        }
                    },
                    event = self.swarm.select_next_some() => {
                        if let SwarmEvent::Behaviour(Event::Gossipsub(libp2p::gossipsub::Event::Message {
                            propagation_source: src,
                            message_id: id,
                            message,
                        })) = event {
                            if self.handler.topics().contains(&message.topic) {
                                let status = self.handler.handle(message);
                                _ = self.swarm
                                    .behaviour_mut()
                                    .gossipsub
                                    .report_message_validation_result(&id, &src, status);
                            }
                        }
                    },
                }
            }
        });

        Ok(())
    }
}
