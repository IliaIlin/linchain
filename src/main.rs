mod lib;

use std::{
    collections::HashMap,
    sync::mpsc::{self, Receiver, Sender},
};

use crate::lib::{InMemoryChannelNetwork, Message, Peer, PeerID};

fn main() {
    let num_of_peers = 3;
    let mut channels: Vec<(Sender<Message>, Receiver<Message>)> = Vec::new();
    for _ in 0..num_of_peers {
        channels.push(mpsc::channel());
    }

    for i in 0..num_of_peers {
        let peer = Peer { id: PeerID(i) };
        let incoming_channel = channels[i as usize].1;
        let mut outgoing_channels = HashMap::new();
        for (idx, &item) in channels.iter().enumerate() {
            if idx != i.try_into().unwrap() {
                outgoing_channels.insert(PeerID(idx as u32), item.0);
            }
        }

        let network = InMemoryChannelNetwork {
            incoming: incoming_channel,
            outgoing: outgoing_channels,
        };
        peer.run(network);
    }
}
