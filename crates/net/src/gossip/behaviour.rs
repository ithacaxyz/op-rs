//! Network Behaviour Module.

use eyre::Result;
use libp2p::{
    gossipsub::{Config, IdentTopic, MessageAuthenticity},
    swarm::NetworkBehaviour,
};

use super::{event::Event, handler::Handler};

/// Specifies the [NetworkBehaviour] of the node
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct Behaviour {
    /// Responds to inbound pings and send outbound pings.
    pub ping: libp2p::ping::Behaviour,
    /// Enables gossipsub as the routing layer.
    pub gossipsub: libp2p::gossipsub::Behaviour,
}

impl Behaviour {
    /// Configures the swarm behaviors, subscribes to the gossip topics, and returns a new
    /// [Behaviour].
    pub fn new(cfg: Config, handlers: &[Box<dyn Handler>]) -> Result<Self> {
        let ping = libp2p::ping::Behaviour::default();

        let mut gossipsub = libp2p::gossipsub::Behaviour::new(MessageAuthenticity::Anonymous, cfg)
            .map_err(|_| eyre::eyre!("gossipsub behaviour creation failed"))?;

        handlers
            .iter()
            .flat_map(|handler| {
                handler
                    .topics()
                    .iter()
                    .map(|topic| {
                        let topic = IdentTopic::new(topic.to_string());
                        gossipsub.subscribe(&topic).map_err(|_| eyre::eyre!("subscription failed"))
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Result<Vec<bool>>>()?;

        Ok(Self { ping, gossipsub })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gossip::{config, handler::BlockHandler};
    use alloy_primitives::Address;
    use libp2p::gossipsub::{IdentTopic, TopicHash};

    fn zero_topics() -> Vec<TopicHash> {
        vec![
            IdentTopic::new("/optimism/0/0/blocks").hash(),
            IdentTopic::new("/optimism/0/1/blocks").hash(),
            IdentTopic::new("/optimism/0/2/blocks").hash(),
        ]
    }

    #[test]
    fn test_behaviour_no_handlers() {
        let cfg = config::default_config_builder().build().expect("Failed to build default config");
        let handlers = vec![];
        let _ = Behaviour::new(cfg, &handlers).unwrap();
    }

    #[test]
    fn test_behaviour_with_handlers() {
        let cfg = config::default_config_builder().build().expect("Failed to build default config");
        let (_, recv) = tokio::sync::watch::channel(Address::default());
        let (block_handler, _) = BlockHandler::new(0, recv);
        let handlers: Vec<Box<dyn Handler>> = vec![Box::new(block_handler)];
        let behaviour = Behaviour::new(cfg, &handlers).unwrap();
        let mut topics = behaviour.gossipsub.topics().cloned().collect::<Vec<TopicHash>>();
        topics.sort();
        assert_eq!(topics, zero_topics());
    }
}
