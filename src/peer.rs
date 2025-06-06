use crate::blockchain::{
    Address, Hash, PublicKey, SignedBlock, SignedTransaction, UnsignedBlock, UnsignedTransaction,
};
use crate::crypto::{sign_ecdsa, verify_ecdsa};
use crate::network::Message::PeerTransaction;
use crate::network::{InMemoryChannelNetwork, Message};
use crate::storage::FileStorage;
use derive_more::Display;
use k256::ecdsa::SigningKey;
use serde_derive::{Deserialize, Serialize};
use std::error::Error;
use std::{collections::HashMap, sync::mpsc::TryRecvError};

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone, Display)]
pub struct PeerID(pub u32);

pub struct Peer {
    pub id: PeerID,
    pub mempool: MemPool,
}

impl Peer {
    pub fn new(id: PeerID, block_size: usize) -> Self {
        Self {
            id,
            mempool: MemPool::new(block_size),
        }
    }

    pub fn run(
        &mut self,
        private_key: redact::Secret<&SigningKey>,
        network: &InMemoryChannelNetwork,
        state: &mut State,
        file_storage: &FileStorage,
    ) {
        loop {
            match network.incoming.try_recv() {
                Ok(msg) => match msg {
                    Message::ClientTransaction(transaction) => {
                        if let Err(e) = self.process_transaction(
                            private_key,
                            transaction,
                            network,
                            state,
                            file_storage,
                        ) {
                            eprintln!("Transaction processing failed due to: {e}");
                        }
                    }
                    PeerTransaction(transaction) => {
                        if !self.mempool.transactions_mem_pool.contains(&transaction) {
                            if let Err(e) = self.process_transaction(
                                private_key,
                                transaction,
                                network,
                                state,
                                file_storage,
                            ) {
                                eprintln!("Transaction processing failed due to: {e}");
                            }
                        }
                    }
                    Message::NewBlock(_) => {
                        unimplemented!("currently out of scope")
                    }
                },
                Err(TryRecvError::Empty) => (),
                Err(error) => eprintln!("{error}"),
            }
        }
    }

    fn process_transaction(
        &mut self,
        private_key: redact::Secret<&SigningKey>,
        transaction: SignedTransaction,
        network: &InMemoryChannelNetwork,
        state: &mut State,
        file_storage: &FileStorage,
    ) -> Result<(), Box<dyn Error>> {
        verify_ecdsa(
            &transaction,
            &*transaction.signature,
            &transaction.public_key,
        )?;
        self.mempool.add_new_transaction(transaction.clone());
        if self.mempool.has_reached_max_size() {
            self.process_new_own_block(private_key, state, network, file_storage)?;
        }
        let errors = network.broadcast_to_all(PeerTransaction(transaction));
        Self::combine_multiple_broadcasting_issues_to_result(errors)
    }

    fn process_new_own_block(
        &mut self,
        private_key: redact::Secret<&SigningKey>,
        state: &mut State,
        network: &InMemoryChannelNetwork,
        file_storage: &FileStorage,
    ) -> Result<(), Box<dyn Error>> {
        let new_signed_block = self.create_and_sign_new_block(private_key, state);
        file_storage
            .save_block(&new_signed_block)
            .expect("Failed to store block");
        state.recalculate_state(&new_signed_block)?;
        let errors = network.broadcast_to_all(Message::NewBlock(new_signed_block));
        Self::combine_multiple_broadcasting_issues_to_result(errors)
    }

    fn combine_multiple_broadcasting_issues_to_result<T: ToString, I: IntoIterator<Item = T>>(
        issues: I,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let issues_vec: Vec<String> = issues.into_iter().map(|e| e.to_string()).collect();
        if issues_vec.is_empty() {
            Ok(())
        } else {
            Err(issues_vec.join(", ").into())
        }
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

// #[cfg(test)]
// mod tests {
//     use crate::blockchain::{Transaction, TransactionInfo};
//     use crate::network::{InMemoryChannelNetwork, Message};
//     use crate::peer::{AssetName, Peer, PeerID, State};
//     use crate::storage::MockStorage;
//     use chrono::{DateTime, Utc};
//     use std::collections::HashMap;
//     use std::sync::mpsc;
//     use std::sync::mpsc::{Receiver, Sender};
//
//     #[test]
//     fn client_sends_transaction_to_peer_signature_verified() {
//         let mut storage = MockStorage::new();
//
//         storage
//             .expect_load_all_blocks()
//             .returning(|| Ok(Vec::new()));
//
//         let state = State::build_from_storage(storage);
//
//         let (tx, rx): (Sender<Message>, Receiver<Message>) = mpsc::channel();
//         let network = InMemoryChannelNetwork {
//             incoming: rx,
//             outgoing: HashMap::new(),
//         };
//         let timestamp: DateTime<Utc> = "2025-05-30T14:00:00Z".parse().unwrap();
//         let transaction = Transaction::ValueTransferTransaction {
//             sender_addr: "sender1".to_string(),
//             receiver_addr: "receiver1".to_string(),
//             info: TransactionInfo {
//                 amount: 3,
//                 asset_name: AssetName::ETH,
//                 timestamp,
//                 signature: "signature1".to_string(),
//             },
//         };
//
//         let peer = Peer::new(PeerID(0), 2);
//
//         let _ = tx.send(Message::ClientTransaction(transaction));
//     }
// }
