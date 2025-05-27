mod lib;

use std::{collections::HashMap, sync::mpsc::{self, Receiver}};

use crate::lib::{InMemoryChannelNetwork, Peer};

fn main() {
    let (tx1, rx1) = mpsc::channel();
    let (tx2, rx2) = mpsc::channel();
    let (tx3, rx3) = mpsc::channel();

    let network_of_first_peer = InMemoryChannelNetwork {
        incoming: rx1,
        outgoing: HashMap::from([(2, tx2), (3, tx3.clone())]),
    };
    let network_of_second_peer = InMemoryChannelNetwork {
        incoming: rx2,
        outgoing: HashMap::from([(1, tx1), (3, tx3.clone())]),
    };
    let firstPeer = Peer {
        id: 1,
        network: network_of_first_peer,
    };
    firstPeer.run();
}
