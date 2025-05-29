#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::mpsc, thread, time::Duration};

    use linchain::{InMemoryChannelNetwork, Message, NetworkError, Peer, PeerID};

    #[test]
    fn network_test_sending_message_to_existent_peer() {
        let (sender, receiver) = mpsc::channel();
        let existing_peer_id = PeerID(1);

        let network = InMemoryChannelNetwork {
            incoming: mpsc::channel().1, // Dummy
            outgoing: HashMap::from([(existing_peer_id, sender)]),
            idle_time_secs: 0,
        };
        let message = Message::PlainText("Test".into());
        network.send(existing_peer_id, message.clone()).unwrap();

        assert_eq!(receiver.recv().unwrap(), message);
    }

    #[test]
    fn network_test_sending_message_to_nonexistent_peer_returns_error() {
        let network = InMemoryChannelNetwork {
            incoming: mpsc::channel().1,
            outgoing: HashMap::new(),
            idle_time_secs: 0,
        };

        let result = network.send(PeerID(999), Message::PlainText("Test".into()));
        assert!(matches!(result, Err(NetworkError::SendFailed)));
    }

    #[test]
    fn peer_test_check_messages_sent_to_outgoing_channels() {
        let (tx1, rx1) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();

        let network = InMemoryChannelNetwork {
            incoming: mpsc::channel().1,
            outgoing: HashMap::from([(PeerID(2), tx1), (PeerID(3), tx2)]),
            idle_time_secs: 0,
        };

        let peer = Peer { id: PeerID(1) };
        let handle = thread::spawn(move || {
            peer.run(network);
        });

        assert!(rx1.recv_timeout(Duration::from_millis(100)).is_ok());
        assert!(rx2.recv_timeout(Duration::from_millis(100)).is_ok());
    }
}
