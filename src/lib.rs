use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender},
    thread,
    thread::sleep,
    time::Duration,
};

use rand::Rng;

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
pub struct PeerID(pub u32);

pub enum Message {
    PlainText(String),
}

pub struct Peer {
    pub id: PeerID,
}

impl Peer {
    pub fn run(&self, network: InMemoryChannelNetwork) {
        let mut random = rand::rng();
        loop {
            let peer_id_to_connect_to =
                PeerID(random.random_range(0..network.outgoing.len() as u32));
            let message = Message::PlainText(format!("Hi from {:?}", peer_id_to_connect_to));
            //thread::spawn(move || {
            match network.send(peer_id_to_connect_to, message) {
                Ok(_) => {
                    println!(
                        "Successfully sent message from {:?} to {:?}",
                        self.id, peer_id_to_connect_to
                    );
                }
                Err(error) => {
                   println!(
                        "Error {:?} sending message from {:?} to {:?}",
                        error, self.id, peer_id_to_connect_to
                    ); 
                }
            };
            //});

            sleep(Duration::from_secs(network.idle_time_secs));
        }
    }
}

#[derive(Debug)]
enum NetworkError {
    SendFailed,
    ReceiveFailed,
}

pub struct InMemoryChannelNetwork {
    pub incoming: Receiver<Message>,
    pub outgoing: HashMap<PeerID, Sender<Message>>,
    pub idle_time_secs: u64
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
