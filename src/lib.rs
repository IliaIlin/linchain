use std::{
    collections::HashMap,
    sync::mpsc::{self, Receiver, Sender},
};

type PeerID = u32;

enum NetworkError {
    SendFailed,
    ReceiveFailed,
}

trait Network {
    fn send(&self, peer_id: PeerID, msg: String) -> Result<(), NetworkError>;
    fn receive(&self) -> Result<String, NetworkError>;
}

pub struct Peer<N: Network> {
    pub(crate) id: PeerID,
    pub(crate) network: N,
}

pub struct InMemoryChannelNetwork {
    pub(crate) incoming: Receiver<String>,
    pub(crate) outgoing: HashMap<PeerID, Sender<String>>,
}

impl<N: Network> Peer<N> {

    pub async fn run(&self) {
        //let mut interval = tokio::time::interval(Duration::from_secs(5));

        loop {
            // tokio::select! {
            //     _ = interval.tick() => self.broadcast_heartbeat().await,
            //     Some((sender, msg)) = self.network.receive().next() => {
            //         self.handle_message(sender, msg).await;
            //     }
            // }
        }
    }

    // async fn handle_message(&mut self, sender: PeerID, msg: N::Message) {
    //     // Common message handling logic
    //     // match msg {
    //     //     // ... message variants
    //     // }
    // }
}

impl Network for InMemoryChannelNetwork {
    fn send(&self, peer_id: PeerID, msg: String) -> Result<(), NetworkError> {
        self.outgoing
            .get(&peer_id)
            .and_then(|sender| sender.send(msg).ok())
            .ok_or(NetworkError::SendFailed)
    }

    fn receive(&self) -> Result<String, NetworkError> {
        self.incoming
            .recv()
            .map(|(msg)| msg) // Destructure (PeerID, String) and keep message
            .map_err(|_| NetworkError::ReceiveFailed)
    }
}

// impl Network for InMemoryChannelNetwork {
//     fn send(&self, peer_id: PeerID, msg: str) -> Result<(), NetworkError> {
//         self.outgoing
//             .get(&peer_id)
//             .ok_or(NetworkError::PeerNotFound)?
//             .send(msg)
//             .map_err(|_| NetworkError::SendFailed)
//     }

//     fn receive(&mut self) -> impl Stream<Item = (PeerID, Message)> {
//         futures::stream::unfold(&mut self.incoming, |rx| async {
//             rx.recv().await.map(|msg| (msg, rx))
//         })
//     }
//}
