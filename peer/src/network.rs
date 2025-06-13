use crate::blockchain::{SignedBlock, SignedTransaction};
use derive_more::From;
use libp2p::gossipsub::{Config, MessageAuthenticity};
use libp2p::swarm::NetworkBehaviour;
use libp2p::{Swarm, gossipsub, mdns, noise, tcp, yamux};
use serde_derive::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    ClientTransaction(SignedTransaction),
    PeerTransaction(SignedTransaction),
    NewBlock(SignedBlock),
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "NetworkEvent")]
pub struct P2PMdnsNetwork {
    pub pub_sub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
}

#[derive(Debug, From)]
pub enum NetworkEvent {
    Mdns(mdns::Event),
    GossipSub(gossipsub::Event),
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
            let pub_sub = gossipsub::Behaviour::new(
                MessageAuthenticity::Signed(key.clone()),
                Config::default(),
            )
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
