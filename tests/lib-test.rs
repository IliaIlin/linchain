// #[cfg(test)]
// mod tests {
//     use std::{
//         collections::HashMap,
//         fs::remove_file,
//         sync::mpsc::{self, Receiver, Sender},
//         thread::{self, sleep},
//         time::Duration,
//     };
//
//     use chrono::{DateTime, Utc};
//     use linchain::{
//         Account, AssetName, Block, InMemoryChannelNetwork, Message, NetworkError, Peer, PeerID,
//         State, Storage, Transaction, TransactionInfo,
//     };
//     use serde_jsonlines::json_lines;
//
//     #[test]
//     fn network_test_sending_message_to_existent_peer() {
//         let (sender, receiver) = mpsc::channel();
//         let existing_peer_id = PeerID(1);
//
//         let network = InMemoryChannelNetwork {
//             incoming: mpsc::channel().1, // Dummy
//             outgoing: HashMap::from([(existing_peer_id, sender)]),
//             idle_time_secs: 0,
//         };
//         let message = Message::PlainText("Test".into());
//         network.send(existing_peer_id, message.clone()).unwrap();
//
//         assert_eq!(receiver.recv().unwrap(), message);
//     }
//
//     #[test]
//     fn network_test_sending_message_to_nonexistent_peer_returns_error() {
//         let network = InMemoryChannelNetwork {
//             incoming: mpsc::channel().1,
//             outgoing: HashMap::new(),
//             idle_time_secs: 0,
//         };
//
//         let result = network.send(PeerID(999), Message::PlainText("Test".into()));
//         assert!(matches!(result, Err(NetworkError::SendFailed)));
//     }
//
//     #[test]
//     #[ignore] // this logic is currently not in use
//     fn peer_test_check_messages_sent_to_outgoing_channels() {
//         let (tx1, rx1) = mpsc::channel();
//         let (tx2, rx2) = mpsc::channel();
//
//         let network = InMemoryChannelNetwork {
//             incoming: mpsc::channel().1,
//             outgoing: HashMap::from([(PeerID(2), tx1), (PeerID(3), tx2)]),
//             idle_time_secs: 0,
//         };
//
//         let mut peer = Peer::new(PeerID(1), 10);
//         let initial_state = State::build_from_storage(Storage::FileStorage {
//             filename: "dummy.json".to_string(),
//         })
//         .unwrap();
//         thread::spawn(move || {
//             peer.run(network, &initial_state);
//         });
//
//         assert!(rx1.recv_timeout(Duration::from_millis(100)).is_ok());
//         assert!(rx2.recv_timeout(Duration::from_millis(100)).is_ok());
//     }
//
//     #[test]
//     fn block_is_stored_in_file() {
//         let (tx, rx): (Sender<Message>, Receiver<Message>) = mpsc::channel();
//
//         let network = InMemoryChannelNetwork {
//             incoming: rx,
//             outgoing: HashMap::new(),
//             idle_time_secs: 0,
//         };
//
//         let timestamp: DateTime<Utc> = "2025-05-30T14:00:00Z".parse().unwrap();
//
//         let input_transactions = vec![
//             Transaction::ValueTransferTransaction {
//                 sender_addr: "sender1".to_string(),
//                 receiver_addr: "receiver1".to_string(),
//                 info: TransactionInfo {
//                     amount: 0.3,
//                     asset_name: AssetName::ETH,
//                     timestamp: timestamp,
//                     signature: "signature1".to_string(),
//                 },
//             },
//             Transaction::ValueTransferTransaction {
//                 sender_addr: "sender2".to_string(),
//                 receiver_addr: "receiver2".to_string(),
//                 info: TransactionInfo {
//                     amount: 0.7,
//                     asset_name: AssetName::BTC,
//                     timestamp: timestamp,
//                     signature: "signature2".to_string(),
//                 },
//             },
//         ];
//
//         for transaction in &input_transactions {
//             let _ = tx.send(Message::TransactionMessage(transaction.clone()));
//         }
//
//         let filename = "test.jsonl";
//         let initial_state = State::build_from_storage(Storage::FileStorage {
//             filename: filename.to_string(),
//         })
//         .unwrap();
//         let mut peer = Peer::new(PeerID(1), 2);
//         thread::spawn(move || {
//             peer.run(network, &initial_state);
//         });
//
//         sleep(Duration::from_millis(1000));
//
//         let blocks_from_file = json_lines(filename)
//             .unwrap()
//             .collect::<Result<Vec<Block>, _>>()
//             .unwrap();
//
//         let transactions_read_from_file = &blocks_from_file.first().unwrap().transactions;
//
//         assert_eq!(input_transactions, *transactions_read_from_file);
//         remove_file(filename).unwrap();
//     }
//
//     #[test]
//     fn initial_state_is_calculated() {
//         let filename = "test.jsonl";
//         let timestamp: DateTime<Utc> = "2025-05-30T14:00:00Z".parse().unwrap();
//
//         let transactions_first_block = vec![
//             Transaction::TopUpTransaction {
//                 receiver_addr: "addr1".to_string(),
//                 info: TransactionInfo {
//                     amount: 5.0,
//                     asset_name: AssetName::ETH,
//                     timestamp: timestamp,
//                     signature: "signature1".to_string(),
//                 },
//             },
//             Transaction::TopUpTransaction {
//                 receiver_addr: "addr2".to_string(),
//                 info: TransactionInfo {
//                     amount: 0.7,
//                     asset_name: AssetName::BTC,
//                     timestamp: timestamp,
//                     signature: "signature1".to_string(),
//                 },
//             },
//         ];
//
//         let transactions_second_block = vec![
//             Transaction::ValueTransferTransaction {
//                 sender_addr: "addr1".to_string(),
//                 receiver_addr: "addr2".to_string(),
//                 info: TransactionInfo {
//                     amount: 2.0,
//                     asset_name: AssetName::ETH,
//                     timestamp: timestamp,
//                     signature: "signature1".to_string(),
//                 },
//             },
//             Transaction::ValueTransferTransaction {
//                 sender_addr: "addr2".to_string(),
//                 receiver_addr: "addr1".to_string(),
//                 info: TransactionInfo {
//                     amount: 0.2,
//                     asset_name: AssetName::BTC,
//                     timestamp: timestamp,
//                     signature: "signature2".to_string(),
//                 },
//             },
//         ];
//
//         let _ = Block::new(transactions_first_block);
//         let _ = Block::new(transactions_second_block);
//
//         let expected_state = vec![
//             Account {
//                 address: "addr1".to_string(),
//                 assets: HashMap::from([(AssetName::ETH, 3.0), (AssetName::BTC, 0.2)]),
//             },
//             Account {
//                 address: "addr2".to_string(),
//                 assets: HashMap::from([(AssetName::ETH, 2.0), (AssetName::BTC, 0.5)]),
//             },
//         ];
//         let actual_state = State::build_from_storage(Storage::FileStorage {
//             filename: filename.to_string(),
//         })
//         .unwrap()
//         .accounts;
//
//         assert_eq!(actual_state, *expected_state);
//         remove_file(filename).unwrap();
//     }
// }
