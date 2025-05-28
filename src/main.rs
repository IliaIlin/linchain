mod lib;

use std::{
    collections::HashMap,
    sync::mpsc::{self},
    thread,
};

use std::array::from_fn;

use crate::lib::{InMemoryChannelNetwork, Peer, PeerID};

const NUM_OF_PEERS: usize = 3;
const PEER_IDLE_SECS_DEFAULT: u64 = 3;

fn main() {
    let channels: [_; NUM_OF_PEERS] = from_fn(|_| mpsc::channel());
    let mut peer_threads = vec![];

    for (current_peer_id, (sender, receiver)) in channels.into_iter().enumerate() {
        let peer = Peer {
            id: PeerID(current_peer_id as u32),
        };
        let mut outgoing_channels = HashMap::new();
        for peer_id in (0..NUM_OF_PEERS).filter(|peer_id| *peer_id != current_peer_id) {
            outgoing_channels.insert(PeerID(peer_id as u32), sender.clone());
        }
        let network = InMemoryChannelNetwork {
            incoming: receiver,
            outgoing: outgoing_channels,
            idle_time_secs: PEER_IDLE_SECS_DEFAULT,
        };
        let peer_thread = thread::spawn(move || {
            peer.run(network);
        });
        peer_threads.push(peer_thread);
    }

    for peer_thread in peer_threads {
        peer_thread.join().unwrap();
    }
}
