mod lib;

use std::{
    sync::mpsc::{self},
    thread,
};

use crate::lib::{InMemoryChannelNetwork, Peer, PeerID};

const NUM_OF_PEERS: usize = 3;
const PEER_IDLE_SECS_DEFAULT: u64 = 10;

fn main() {
    let (sender_channels, receiver_channels): (Vec<_>, Vec<_>) =
        (0..NUM_OF_PEERS).map(|_| mpsc::channel()).unzip();
    let mut peer_threads = vec![];

    for (peer_id, receiver) in receiver_channels.into_iter().enumerate() {
        let peer = Peer {
            id: PeerID(peer_id as u32),
        };
        let outgoing_channels = sender_channels.clone()
            .into_iter()
            .enumerate()
            .filter(|&(i, _)| i != peer_id)
            .map(|(i, s)| (PeerID(i as u32), s.clone()))
            .collect();

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
