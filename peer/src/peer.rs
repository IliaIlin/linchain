use crate::blockchain::{
    Address, Hash, PublicKey, SignedBlock, SignedTransaction, UnsignedBlock, UnsignedTransaction,
};
use crate::crypto::{sign_ecdsa, verify_ecdsa};
use crate::network::Message::{NewBlock, PeerTransaction};
use crate::network::{Message, NetworkEvent, P2PMdnsNetwork};
use crate::storage::FileStorage;
use derive_more::Display;
use futures::StreamExt;
use k256::ecdsa::SigningKey;
use libp2p::gossipsub::IdentTopic;
use libp2p::swarm::SwarmEvent;
use libp2p::{gossipsub, mdns, Swarm};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone, Display)]
pub struct PeerID(pub u32);

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
    ) {
        let mut dialing_peers = HashSet::new();

        loop {
            match swarm.select_next_some().await {
                SwarmEvent::Behaviour(NetworkEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, addr) in list {
                        if !swarm.is_connected(&peer_id) && !dialing_peers.contains(&peer_id) {
                            println!("Discovered peer: {}, dialing...", peer_id);
                            dialing_peers.insert(peer_id.clone());
                            let _ = swarm.dial(addr.clone());
                        }
                    }
                }
                SwarmEvent::Behaviour(NetworkEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _) in list {
                        swarm
                            .disconnect_peer_id(peer_id)
                            .expect("Panicking at peer disconnect");
                    }
                }
                SwarmEvent::Behaviour(NetworkEvent::GossipSub(gossipsub::Event::Message {
                    propagation_source,
                    message_id: _message_id,
                    message,
                })) => {
                    println!(
                        "Received from {:?} on {:?}",
                        propagation_source, message.topic
                    );
                    let unwrapped_message = match bcs::from_bytes::<Message>(&*message.data) {
                        Ok(decoded) => decoded,
                        Err(e) => {
                            println!("Failed to decode message: {e}");
                            continue;
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
                                eprintln!("Transaction processing failed due to: {e}");
                            } else {
                                println!("Transaction processed successfully");
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
                                    eprintln!("Transaction processing failed due to: {e}");
                                }
                            }
                        }
                        NewBlock(_) => {
                            unimplemented!("currently out of scope")
                        }
                    }
                }
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {:?}", address);
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    dialing_peers.remove(&peer_id);
                    swarm
                        .behaviour_mut()
                        .pub_sub
                        .subscribe(&topic)
                        .expect("Failed to subscribe");
                    println!("Connected to: {}", peer_id);
                }
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    dialing_peers.remove(&peer_id);
                    swarm.behaviour_mut().pub_sub.unsubscribe(&topic);
                    println!("Disconnected from: {}", peer_id);
                }
                _ => {}
            }
        }
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
            &transaction,
            &*transaction.signature,
            &transaction.public_key,
        )?;
        self.mempool.add_new_transaction(transaction.clone());
        if self.mempool.has_reached_max_size() {
            self.process_new_own_block(private_key, state, file_storage, swarm, topic)?;
        }
        let transaction_bytes = bcs::to_bytes(&PeerTransaction(transaction))?;
        swarm
            .behaviour_mut()
            .pub_sub
            .publish(topic.clone(), transaction_bytes)
            .map(|_| Ok(()))
            .map_err(|e| Box::new(e) as Box<dyn Error>)?
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

        for account in accounts_to_update {
            let existing_account = self
                .accounts
                .iter_mut()
                .find(|acc| acc.address == account.address)
                .ok_or_else(|| format!("Account {:?} not found", account.address))?;

            for (asset, amount) in account.assets {
                *existing_account.assets.entry(asset).or_insert(0) += amount;
            }
        }
        Ok(())
    }
}
