use crate::blockchain::{SignedBlock, SignedTransaction};
use crate::peer::PeerID;
use libp2p::gossipsub::{Behaviour, Config, MessageAuthenticity};
use libp2p::swarm::NetworkBehaviour;
use libp2p::{Swarm, gossipsub, mdns, noise, tcp, yamux};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc::{Receiver, RecvError, SendError, Sender};
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

pub struct InMemoryChannelNetwork {
    pub incoming: Receiver<Message>,
    pub outgoing: HashMap<PeerID, Sender<Message>>,
}

impl InMemoryChannelNetwork {
    pub fn send(&self, peer_id: PeerID, msg: Message) -> Result<(), NetworkError> {
        self.outgoing
            .get(&peer_id)
            .ok_or(NetworkError::PeerNotFound { peer_id })?
            .send(msg)
            .map_err(|e| NetworkError::SendToPeerFailed { peer_id, cause: e })
    }

    pub fn receive(&self) -> Result<Message, NetworkError> {
        self.incoming
            .recv()
            .map_err(|e| NetworkError::ReceiveFailed { cause: e })
    }

    pub fn broadcast_to_all(&self, msg: Message) -> Vec<NetworkError> {
        let mut errors = Vec::new();
        for peer_id in self.outgoing.keys() {
            match self.send(*peer_id, msg.clone()) {
                Ok(_) => (),
                Err(e) => errors.push(e),
            }
        }
        errors
    }
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
    Application(Message),
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

impl From<Message> for NetworkEvent {
    fn from(msg: Message) -> Self {
        NetworkEvent::Application(msg)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{SendError, channel};

    #[test]
    fn test_display_texts_of_errors() {
        let err = NetworkError::PeerNotFound { peer_id: PeerID(1) };
        assert_eq!(format!("{}", err), "Peer with id=1 not found in network");

        let msg = Message::ClientTransaction(SignedTransaction::default());
        let err = NetworkError::SendToPeerFailed {
            peer_id: PeerID(2),
            cause: SendError(msg),
        };
        assert_eq!(
            format!("{}", err),
            "Can't send the message to peer with id=2 due to: sending on a closed channel"
        );

        let err = NetworkError::ReceiveFailed { cause: RecvError };
        assert_eq!(
            format!("{}", err),
            "Can't receive the message due to: receiving on a closed channel"
        );
    }

    #[test]
    fn test_send_success() {
        let outgoing_peer_id = PeerID(1);
        let (sender_for_outgoing_peer, receiver_for_outgoing_peer) = channel();
        let (_, incoming_receiver) = channel();
        let network = InMemoryChannelNetwork {
            incoming: incoming_receiver,
            outgoing: HashMap::from([(outgoing_peer_id, sender_for_outgoing_peer)]),
        };

        let msg = Message::ClientTransaction(SignedTransaction::default());
        assert!(network.send(outgoing_peer_id, msg.clone()).is_ok());

        let received_msg = receiver_for_outgoing_peer.recv().unwrap();
        assert_eq!(received_msg, msg);
    }

    #[test]
    fn test_send_to_non_existing_peer() {
        let outgoing_peer_id = PeerID(1);
        let (_, incoming_receiver) = channel();
        let network = InMemoryChannelNetwork {
            incoming: incoming_receiver,
            outgoing: HashMap::new(),
        };

        let result = network
            .send(
                outgoing_peer_id,
                Message::ClientTransaction(SignedTransaction::default()),
            )
            .unwrap_err();
        assert!(match result {
            NetworkError::PeerNotFound { peer_id } if peer_id == outgoing_peer_id => true,
            _ => false,
        });
    }

    #[test]
    fn test_send_to_closed_channel_returns_send_error() {
        let outgoing_peer_id = PeerID(1);
        let (sender_for_outgoing_peer, receiver_for_outgoing_peer) = channel();
        drop(receiver_for_outgoing_peer); // Close the channel immediately
        let (_, incoming_receiver) = channel();
        let network = InMemoryChannelNetwork {
            incoming: incoming_receiver,
            outgoing: HashMap::from([(outgoing_peer_id, sender_for_outgoing_peer)]),
        };

        let msg = Message::ClientTransaction(SignedTransaction::default());
        let result = network.send(outgoing_peer_id, msg.clone()).unwrap_err();
        assert!(match result {
            NetworkError::SendToPeerFailed {
                peer_id,
                cause: SendError(_),
            } if peer_id == outgoing_peer_id => true,
            _ => false,
        });
    }
}
