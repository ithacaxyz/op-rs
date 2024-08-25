//! Driver for p2p services.

use alloy::primitives::Address;
use eyre::Result;
use futures::stream::StreamExt;
use libp2p::{swarm::SwarmEvent, Multiaddr, SwarmBuilder};
use libp2p_identity::Keypair;
use std::net::SocketAddr;
use tokio::{
    select,
    sync::watch::{Receiver, Sender},
};

use crate::{
    behaviour::Behaviour,
    event::Event,
    handler::{BlockHandler, Handler},
    types::{ExecutionPayloadEnvelope, NetworkAddress},
};

/// Driver contains the logic for the P2P service.
pub struct GossipDriver {
    /// The [Behaviour] of the node.
    pub behaviour: Behaviour,
    /// Channel to receive unsafe blocks.
    pub unsafe_block_recv: Receiver<ExecutionPayloadEnvelope>,
    /// Channel to send unsafe signer updates.
    pub unsafe_block_signer_sender: Sender<Address>,
    /// The socket address that the service is listening on.
    pub addr: SocketAddr,
    /// Block handler.
    pub handler: BlockHandler,
    /// The chain ID of the network.
    pub chain_id: u64,
    /// A unique keypair to validate the node's identity
    pub keypair: Keypair,
}

impl GossipDriver {
    /// Starts the Discv5 peer discovery & libp2p services
    /// and continually listens for new peers and messages to handle
    pub fn start(self) -> Result<()> {
        // TODO: pull this swarm building out into the builder
        let mut swarm = SwarmBuilder::with_existing_identity(self.keypair.clone())
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )?
            .with_behaviour(|_| self.behaviour)?
            .build();
        let addr = NetworkAddress::try_from(self.addr)?;
        // let mut peer_recv = discovery::start(addr, self.chain_id)?;
        let multiaddr = Multiaddr::from(addr);
        swarm.listen_on(multiaddr).map_err(|_| eyre::eyre!("swarm listen failed"))?;
        let handler = self.handler.clone();
        tokio::spawn(async move {
            loop {
                select! {
                    // peer = peer_recv.recv().fuse() => {
                    //     if let Some(peer) = peer {
                    //         let peer = Multiaddr::from(peer);
                    //         _ = swarm.dial(peer);
                    //     }
                    // },
                    event = swarm.select_next_some() => {
                        if let SwarmEvent::Behaviour(Event::Gossipsub(libp2p::gossipsub::Event::Message {
                            propagation_source: _peer_id,
                            message_id: _id,
                            message,
                        })) = event {
                            if handler.topics().contains(&message.topic) {
                                let _status = handler.handle(message);
                                // TODO: report message validation result??
                                // _ = swarm
                                //     .behaviour_mut()
                                //     .report_message_validation_result(&message_id, &propagation_source, status);
                            }
                        }
                    },
                }
            }
        });

        Ok(())
    }
}
