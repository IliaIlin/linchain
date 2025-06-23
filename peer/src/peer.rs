use crate::blockchain::{
    Address, Hash, PublicKey, SignedBlock, SignedTransaction, UnsignedBlock, UnsignedTransaction,
};
use crate::crypto::{sign_ecdsa, verify_ecdsa};
use crate::network::Message::{Heartbeat, LeaderProposal, LeaderVote, NewBlock, PeerTransaction};
use crate::network::{Message, NetworkEvent, P2PMdnsNetwork};
use crate::storage::FileStorage;
use futures::StreamExt;
use k256::ecdsa::SigningKey;
use libp2p::gossipsub::IdentTopic;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm, gossipsub, mdns};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time;

pub struct Peer {
    pub mempool: MemPool,
}

impl Peer {
    pub fn new(block_size: usize) -> Self {
        Self {
            mempool: MemPool::new(block_size),
        }
    }

    pub async fn run(
        &mut self,
        private_key: redact::Secret<&SigningKey>,
        state: &mut State,
        file_storage: &FileStorage,
        mut swarm: Swarm<P2PMdnsNetwork>,
        topic: IdentTopic,
        mut leader_election: LeaderElection,
    ) {
        let next_election_if_leader_inactive_in_millis = Duration::from_millis(3000);
        let mut heartbeat_timer = time::interval(Duration::from_millis(1000));
        let mut election_check_timer = time::interval(Duration::from_millis(1000));

        loop {
            tokio::select! {
            _ = heartbeat_timer.tick() => {
                match leader_election.state {
                    PeerState::Leader => {
                        let heartbeat_msg = Heartbeat {
                            term: leader_election.current_term,
                            leader_id: *swarm.local_peer_id()
                        };
                            let serialized_result = match bcs::to_bytes(&heartbeat_msg) {
                                    Ok(serialized) => Some(serialized),
                                    Err(e) => {
                                        eprintln!("Heartbeat failed due to {:?}", e);
                                        None
                                    }
                                };
                            if serialized_result.is_some() {
                                _ = Self::publish_to_topic(&mut swarm, &topic, serialized_result.unwrap());
                            }
                    }
                        _ => {}
                    }
                    }
                    _ = election_check_timer.tick() => {
                    match leader_election.state {
                        PeerState::Follower | PeerState::Candidate { .. }=> {
                            if leader_election.last_heartbeat.elapsed()>next_election_if_leader_inactive_in_millis{
                               if let Err(e) = Self::start_election(&mut swarm, &topic, &mut leader_election){
                                    eprintln!("Election start failed due to {:?}", e);
                                }
                            };
                        }
                        _ => {}
                }
            }
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(NetworkEvent::Mdns(mdns::Event::Discovered(list))) => {
                            Self::process_peer_discovery(&mut swarm, list);
                        }
                        SwarmEvent::Behaviour(NetworkEvent::Mdns(mdns::Event::Expired(list))) => {
                            Self::process_peer_expiry(&mut swarm, list);
                        }
                        SwarmEvent::Behaviour(NetworkEvent::GossipSub(gossipsub::Event::Message {
                            propagation_source: _propagation_source,
                            message_id: _message_id,
                            message,
                        })) => {
                            println!("Received message from topic: {:?}", topic.to_string());
                            if self.process_message_from_topic(
                                private_key,
                                state,
                                file_storage,
                                &mut swarm,
                                &topic,
                                message,
                                &mut leader_election
                            ) {
                                continue;
                            }
                        }
                        SwarmEvent::NewListenAddr { address, .. } => {
                            println!("Listening on {:?}", address);
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            Self::process_peer_connect(&mut swarm, &topic, &peer_id);
                        }
                        SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            Self::process_peer_disconnect(&mut swarm, &topic, &peer_id);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn process_peer_discovery(swarm: &mut Swarm<P2PMdnsNetwork>, list: Vec<(PeerId, Multiaddr)>) {
        for (peer_id, addr) in list {
            if !swarm.is_connected(&peer_id) {
                println!("Discovered peer: {}, dialing...", peer_id);
                let _ = swarm.dial(addr.clone());
            }
        }
    }

    fn process_peer_expiry(swarm: &mut Swarm<P2PMdnsNetwork>, list: Vec<(PeerId, Multiaddr)>) {
        for (peer_id, _) in list {
            swarm
                .disconnect_peer_id(peer_id)
                .expect("Panicking at peer disconnect");
        }
    }

    fn process_peer_connect(
        swarm: &mut Swarm<P2PMdnsNetwork>,
        topic: &IdentTopic,
        peer_id: &PeerId,
    ) {
        swarm
            .behaviour_mut()
            .pub_sub
            .subscribe(&topic)
            .expect("Failed to subscribe");
        println!("Connected to: {}", peer_id);
    }

    fn process_peer_disconnect(
        swarm: &mut Swarm<P2PMdnsNetwork>,
        topic: &IdentTopic,
        peer_id: &PeerId,
    ) {
        swarm.behaviour_mut().pub_sub.unsubscribe(&topic);
        println!("Disconnected from: {}", peer_id);
    }

    fn process_message_from_topic(
        &mut self,
        private_key: redact::Secret<&SigningKey>,
        state: &mut State,
        file_storage: &FileStorage,
        mut swarm: &mut Swarm<P2PMdnsNetwork>,
        topic: &IdentTopic,
        message: gossipsub::Message,
        leader_election: &mut LeaderElection,
    ) -> bool {
        let unwrapped_message = match bcs::from_bytes::<Message>(&*message.data) {
            Ok(decoded) => decoded,
            Err(e) => {
                println!("Failed to decode message: {e}");
                return true;
            }
        };
        match unwrapped_message {
            Message::ClientTransaction(transaction) => {
                if let Err(e) = self.process_transaction(
                    private_key,
                    transaction,
                    state,
                    file_storage,
                    &mut swarm,
                    &topic,
                ) {
                    eprintln!("Client transaction processing failed due to: {e}");
                } else {
                    println!("Client transaction processed successfully");
                }
            }
            PeerTransaction(transaction) => {
                if !self.mempool.transactions_mem_pool.contains(&transaction) {
                    if let Err(e) = self.process_transaction(
                        private_key,
                        transaction,
                        state,
                        file_storage,
                        &mut swarm,
                        &topic,
                    ) {
                        eprintln!("Peer transaction processing failed due to: {e}");
                    } else {
                        println!("Peer transaction processed successfully");
                    }
                }
            }
            NewBlock(_) => {
                println!("New block received, but no-op impl currently");
            }
            Heartbeat { term, leader_id } => {
                println!("Heartbeat received from: {} for term {}", leader_id, term);
                Self::process_heartbeat(term, leader_election);
            }
            LeaderProposal { term, candidate_id } => {
                println!(
                    "Leader proposal received from: {} for term {}",
                    candidate_id, term
                );
                if let Err(e) =
                    Self::process_leader_proposal(term, swarm, topic, leader_election)
                {
                    eprintln!("Leader proposal processing failed due to: {e}");
                }
            }
            LeaderVote {
                term,
                voter_id,
                granted,
            } => {
                println!(
                    "Leader vote received from: {} for term {} with vote: {}",
                    voter_id, term, granted
                );
                if let Err(e) =
                    Self::process_leader_vote(term, granted, swarm, topic, leader_election)
                {
                    eprintln!("Leader vote processing failed due to: {e}");
                }
            }
        }
        false
    }

    fn process_transaction(
        &mut self,
        private_key: redact::Secret<&SigningKey>,
        transaction: SignedTransaction,
        state: &mut State,
        file_storage: &FileStorage,
        swarm: &mut Swarm<P2PMdnsNetwork>,
        topic: &IdentTopic,
    ) -> Result<(), Box<dyn Error>> {
        verify_ecdsa(
            &transaction.transaction,
            &*transaction.signature,
            &transaction.public_key,
        )?;
        self.mempool.add_new_transaction(transaction.clone());
        if self.mempool.has_reached_max_size() {
            self.process_new_own_block(private_key, state, file_storage, swarm, topic)?;
        }
        let transaction_bytes = bcs::to_bytes(&PeerTransaction(transaction))?;
        Self::publish_to_topic(swarm, topic, transaction_bytes)
    }

    fn process_new_own_block(
        &mut self,
        private_key: redact::Secret<&SigningKey>,
        state: &mut State,
        file_storage: &FileStorage,
        swarm: &mut Swarm<P2PMdnsNetwork>,
        topic: &IdentTopic,
    ) -> Result<(), Box<dyn Error>> {
        let new_signed_block = self.create_and_sign_new_block(private_key, state);
        file_storage
            .save_block(&new_signed_block)
            .expect("Failed to store block");
        state.recalculate_state(&new_signed_block)?;

        let new_block_bytes = bcs::to_bytes(&NewBlock(new_signed_block))?;
        Self::publish_to_topic(swarm, topic, new_block_bytes)
    }

    fn publish_to_topic(
        swarm: &mut Swarm<P2PMdnsNetwork>,
        topic: &IdentTopic,
        new_block_bytes: Vec<u8>,
    ) -> Result<(), Box<dyn Error>> {
        swarm
            .behaviour_mut()
            .pub_sub
            .publish(topic.clone(), new_block_bytes)
            .map(|_| Ok(()))
            .map_err(|e| Box::new(e) as Box<dyn Error>)?
    }

    fn create_and_sign_new_block(
        &mut self,
        private_key: redact::Secret<&SigningKey>,
        state: &State,
    ) -> SignedBlock {
        let transactions = self.mempool.drain_all();
        let new_unsigned_block = UnsignedBlock::new(transactions, state.hash_of_last_block);
        let private_key_exposed = private_key.expose_secret();
        let signature = sign_ecdsa(&new_unsigned_block, private_key_exposed).unwrap();
        let public_key: PublicKey = private_key_exposed.verifying_key().to_sec1_bytes().to_vec();
        SignedBlock {
            block: new_unsigned_block,
            signature,
            public_key,
        }
    }

    fn start_election(
        swarm: &mut Swarm<P2PMdnsNetwork>,
        topic: &IdentTopic,
        leader_election: &mut LeaderElection,
    ) -> Result<(), Box<dyn Error>> {
        leader_election.current_term += 1;
        leader_election.voted = true;
        leader_election.state = PeerState::Candidate { votes_received: 1 }; // selfish voting
        let vote_request_message = LeaderProposal {
            term: leader_election.current_term,
            candidate_id: *swarm.local_peer_id(),
        };
        let serialized_result = bcs::to_bytes(&vote_request_message)?;
        Self::publish_to_topic(swarm, topic, serialized_result)?;
        Ok(())
    }

    fn process_leader_proposal(
        term: u64,
        swarm: &mut Swarm<P2PMdnsNetwork>,
        topic: &IdentTopic,
        leader_election: &mut LeaderElection,
    ) -> Result<(), Box<dyn Error>> {
        let proposal_accepted = if term > leader_election.current_term {
            leader_election.current_term = term;
            leader_election.state = PeerState::Follower;
            leader_election.voted = true;
            true
        } else if term == leader_election.current_term && !leader_election.voted {
            leader_election.voted = true;
            true
        } else {
            false
        };
        let message = LeaderVote {
            term: leader_election.current_term,
            voter_id: *swarm.local_peer_id(),
            granted: proposal_accepted,
        };
        let serialized_result = bcs::to_bytes(&message)?;
        Self::publish_to_topic(swarm, topic, serialized_result)?;

        Ok(())
    }

    fn process_leader_vote(
        term: u64,
        vote_granted: bool,
        swarm: &mut Swarm<P2PMdnsNetwork>,
        topic: &IdentTopic,
        leader_election: &mut LeaderElection,
    ) -> Result<(), Box<dyn Error>> {
        if term != leader_election.current_term {
            return Ok(());
        }
        if let PeerState::Candidate { votes_received } = leader_election.state {
            if vote_granted {
                let new_vote_count = votes_received + 1;
                let all_peers = swarm.connected_peers().count() + 1;
                if new_vote_count >= LeaderElection::majority_votes(all_peers as u8) {
                    leader_election.state = PeerState::Leader;
                    println!("I am elected as a new leader!");
                    let message = Heartbeat {
                        term: leader_election.current_term,
                        leader_id: *swarm.local_peer_id(),
                    };
                    let serialized_result = bcs::to_bytes(&message)?;
                    Self::publish_to_topic(swarm, topic, serialized_result)?
                } else {
                    leader_election.state = PeerState::Candidate {
                        votes_received: new_vote_count,
                    };
                }
            }
        }
        Ok(())
    }

    fn process_heartbeat(term: u64, leader_election: &mut LeaderElection) {
        if term >= leader_election.current_term {
            leader_election.current_term = term;
            leader_election.state = PeerState::Follower;
            leader_election.last_heartbeat = Instant::now();
            leader_election.voted = false;
        }
    }
}

pub struct MemPool {
    transactions_mem_pool: Vec<SignedTransaction>,
    max_size: usize,
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Serialize, Deserialize)]
pub enum AssetName {
    BTC,
    ETH,
}

#[derive(Debug, PartialEq)]
pub struct Account {
    pub address: Address,
    pub assets: HashMap<AssetName, u64>,
}

pub struct State {
    pub accounts: Vec<Account>,
    pub hash_of_last_block: Hash,
}

impl MemPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            transactions_mem_pool: Vec::new(),
            max_size,
        }
    }

    pub fn add_new_transaction(&mut self, transaction: SignedTransaction) {
        self.transactions_mem_pool.push(transaction);
    }

    pub fn has_reached_max_size(&self) -> bool {
        self.transactions_mem_pool.len() == self.max_size
    }

    pub fn drain_all(&mut self) -> Vec<SignedTransaction> {
        self.transactions_mem_pool.drain(0..self.max_size).collect()
    }
}

impl State {
    const ZERO_HASH: Hash = [0; 32];
    pub fn initialize(blocks: Vec<SignedBlock>) -> Result<State, Box<dyn Error>> {
        let hash_of_last_block = blocks
            .last()
            .map(|block| block.block.hash)
            .unwrap_or(Self::ZERO_HASH);
        let accounts = Self::convert_blocks_to_accounts(blocks)?;
        Ok(State {
            accounts,
            hash_of_last_block,
        })
    }

    fn convert_blocks_to_accounts(
        blocks: Vec<SignedBlock>,
    ) -> Result<Vec<Account>, Box<dyn Error>> {
        let mut addresses_to_balances: HashMap<Address, HashMap<AssetName, u64>> = HashMap::new();
        for block in blocks {
            for transaction in block.block.transactions {
                match transaction.transaction {
                    UnsignedTransaction::TopUpTransaction {
                        receiver_addr,
                        info,
                    } => {
                        let balances_for_address = addresses_to_balances
                            .entry(receiver_addr)
                            .or_insert_with(HashMap::new);
                        *balances_for_address.entry(info.asset_name).or_insert(0) += info.amount;
                    }
                    UnsignedTransaction::ValueTransferTransaction {
                        sender_addr,
                        receiver_addr,
                        info,
                    } => {
                        let balances_for_receiver_address = addresses_to_balances
                            .entry(receiver_addr)
                            .or_insert_with(HashMap::new);
                        *balances_for_receiver_address
                            .entry(info.asset_name.clone())
                            .or_insert(0) += info.amount;
                        let balances_for_sender_address = addresses_to_balances
                            .entry(sender_addr)
                            .or_insert_with(HashMap::new);
                        *balances_for_sender_address
                            .entry(info.asset_name)
                            .or_insert(0) -= info.amount;
                    }
                }
            }
        }
        Ok(addresses_to_balances
            .iter()
            .map(|entry| Account {
                address: entry.0.clone(),
                assets: entry.1.clone(),
            })
            .collect())
    }

    fn recalculate_state(&mut self, new_block: &SignedBlock) -> Result<(), Box<dyn Error>> {
        let accounts_to_update = Self::convert_blocks_to_accounts(vec![new_block.clone()])?;

        for mut account in accounts_to_update {
            if let Some(existing_account) = self
                .accounts
                .iter_mut()
                .find(|acc| acc.address == account.address)
            {
                for (asset, amount) in account.assets.drain() {
                    *existing_account.assets.entry(asset).or_insert(0) += amount;
                }
            } else {
                self.accounts.push(account);
            }
        }
        Ok(())
    }
}

pub struct LeaderElection {
    pub current_term: u64,
    pub state: PeerState,
    pub voted: bool,
    pub last_heartbeat: Instant,
}

pub enum PeerState {
    Follower,
    Candidate { votes_received: u8 },
    Leader,
}

impl LeaderElection {
    pub fn new() -> Self {
        Self {
            current_term: 0,
            state: PeerState::Follower,
            voted: false,
            last_heartbeat: Instant::now(),
        }
    }

    pub fn majority_votes(current_num_of_peers: u8) -> u8 {
        (current_num_of_peers / 2) + 1
    }
}
