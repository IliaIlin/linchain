const BLOCK_SIZE: usize = 2;

fn main() {
    println!("I am happily running");
}

// fn simulate_peer_network() {
//     const NUM_OF_PEERS: usize = 3;
//
//     let (sender_channels, receiver_channels): (Vec<_>, Vec<_>) =
//         (0..NUM_OF_PEERS).map(|_| mpsc::channel()).unzip();
//     let mut peer_threads = vec![];
//
//     for (peer_id, receiver) in receiver_channels.into_iter().enumerate() {
//         let mut peer = Peer::new(PeerID(peer_id as u32), BLOCK_SIZE);
//
//         let outgoing_channels = sender_channels
//             .clone()
//             .into_iter()
//             .enumerate()
//             .filter(|&(i, _)| i != peer_id)
//             .map(|(i, s)| (PeerID(i as u32), s.clone()))
//             .collect();
//
//         let network = InMemoryChannelNetwork {
//             incoming: receiver,
//             outgoing: outgoing_channels,
//         };
//         let initial_state =
//             State::build_from_storage(Storage::new("blockchain.json".to_string())).unwrap();
//         let peer_thread = thread::spawn(move || {
//             peer.run(network, &initial_state);
//         });
//         peer_threads.push(peer_thread);
//     }
//
//     for peer_thread in peer_threads {
//         peer_thread.join().unwrap();
//     }
// }
//
// fn simulate_transactions() {
//     let (tx, rx): (Sender<Message>, Receiver<Message>) = mpsc::channel();
//     let mut peer = Peer::new(PeerID(1), BLOCK_SIZE);
//
//     let network = InMemoryChannelNetwork {
//         incoming: rx,
//         outgoing: HashMap::new(),
//     };
//
//     for _ in 0..4 {
//         let _ = tx.send(Message::ClientTransaction(
//             Transaction::ValueTransferTransaction {
//                 sender_addr: "sender1".to_string(),
//                 receiver_addr: "receiver1".to_string(),
//                 info: TransactionInfo {
//                     amount: 3,
//                     asset_name: AssetName::ETH,
//                     timestamp: Utc::now(),
//                     signature: "signature".to_string(),
//                 },
//             },
//         ));
//     }
//
//     let initial_state =
//         State::build_from_storage(Storage::new("blockchain.jsonl".to_string())).unwrap();
//     peer.run(network, &initial_state);
// }
