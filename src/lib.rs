use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender}, thread::sleep, time::Duration,
};

use rand::RngCore;

#[derive(Eq, Hash, PartialEq, Debug)]
pub struct PeerID(pub u32);

pub struct Message(String);

pub struct Peer {
    pub id: PeerID,
}

impl Peer {
    pub fn run(&self, _network: InMemoryChannelNetwork) {
        loop {
            let millis = rand::rng().next_u64();
            sleep(Duration::from_millis(millis));
            println!("Peer with id={:?} is active now", self.id)
        }
    }
}

enum NetworkError {
    SendFailed,
    ReceiveFailed,
}

pub struct InMemoryChannelNetwork {
    pub incoming: Receiver<Message>,
    pub outgoing: HashMap<PeerID, Sender<Message>>,
}

impl InMemoryChannelNetwork {
    fn send(&self, peer_id: PeerID, msg: Message) -> Result<(), NetworkError> {
        self.outgoing
            .get(&peer_id)
            .and_then(|sender| sender.send(msg).ok())
            .ok_or(NetworkError::SendFailed)
    }

    fn receive(&self) -> Result<Message, NetworkError> {
        self.incoming
            .recv() 
            .map_err(|_| NetworkError::ReceiveFailed)
    }
}
