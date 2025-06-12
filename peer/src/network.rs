use crate::blockchain::{SignedBlock, SignedTransaction};
use crate::peer::PeerID;
use libp2p::gossipsub::{Behaviour, Config, MessageAuthenticity};
use libp2p::swarm::NetworkBehaviour;
use libp2p::{Swarm, gossipsub, mdns, noise, tcp, yamux};
use serde_derive::{Deserialize, Serialize};
use std::error::Error;
use std::sync::mpsc::{RecvError, SendError};
use std::time::Duration;
use thiserror::Error;

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    ClientTransaction(SignedTransaction),
    PeerTransaction(SignedTransaction),
    NewBlock(SignedBlock),
}

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Peer with id={peer_id} not found in network")]
    PeerNotFound { peer_id: PeerID },
    #[error("Can't send the message to peer with id={peer_id} due to: {cause}")]
    SendToPeerFailed {
        peer_id: PeerID,
        #[source]
        cause: SendError<Message>,
    },
    #[error("Can't receive the message due to: {cause}")]
    ReceiveFailed {
        #[source]
        cause: RecvError,
    },
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "NetworkEvent")]
pub struct P2PMdnsNetwork {
    pub pub_sub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
}

#[derive(Debug)]
pub enum NetworkEvent {
    Mdns(mdns::Event),
    GossipSub(gossipsub::Event),
}

impl From<mdns::Event> for NetworkEvent {
    fn from(event: mdns::Event) -> Self {
        NetworkEvent::Mdns(event)
    }
}
impl From<gossipsub::Event> for NetworkEvent {
    fn from(event: gossipsub::Event) -> Self {
        NetworkEvent::GossipSub(event)
    }
}

pub fn build_p2p_network_swarm(
    stay_alive_secs: u64,
) -> Result<Swarm<P2PMdnsNetwork>, Box<dyn Error>> {
    let swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| {
            let pub_sub =
                Behaviour::new(MessageAuthenticity::Signed(key.clone()), Config::default())
                    .expect("Can't create gossipsub behaviour");
            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;
            Ok(P2PMdnsNetwork { pub_sub, mdns })
        })?
        .with_swarm_config(|cfg| {
            cfg.with_idle_connection_timeout(Duration::from_secs(stay_alive_secs))
        })
        .build();
    Ok(swarm)
}
