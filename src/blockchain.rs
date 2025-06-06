use crate::peer::AssetName;
use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type Hash = [u8; 32];
pub type PublicKey = Vec<u8>;

#[derive(Eq, Hash, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Address(PublicKey);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct SignedTransaction {
    #[serde(flatten)]
    pub transaction: UnsignedTransaction,
    pub signature: String,
    pub public_key: PublicKey,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum UnsignedTransaction {
    ValueTransferTransaction {
        sender_addr: Address,
        receiver_addr: Address,
        #[serde(flatten)]
        info: TransactionInfo,
    },
    TopUpTransaction {
        receiver_addr: Address,
        #[serde(flatten)]
        info: TransactionInfo,
    },
}
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    pub amount: u64,
    pub asset_name: AssetName,
    #[serde(with = "chrono::serde::ts_nanoseconds")]
    pub timestamp: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SignedBlock {
    #[serde(flatten)]
    pub block: UnsignedBlock,
    pub signature: String,
    pub public_key: PublicKey,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct UnsignedBlock {
    pub transactions: Vec<SignedTransaction>,
    pub hash: Hash,
    pub previous_block_hash: Hash,
}

impl UnsignedBlock {
    pub fn new(transactions: Vec<SignedTransaction>, previous_block_hash: Hash) -> Self {
        Self {
            hash: Self::calculate_hash(&transactions, &previous_block_hash),
            previous_block_hash,
            transactions,
        }
    }

    fn calculate_hash(transactions: &Vec<SignedTransaction>, previous_block_hash: &Hash) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update(previous_block_hash);
        hasher.update(bcs::to_bytes(transactions).unwrap());
        hasher.finalize().into()
    }
}

//TODO replace with proptests generators.
#[cfg(test)]
impl Default for SignedTransaction {
    fn default() -> Self {
        SignedTransaction {
            transaction: UnsignedTransaction::default(),
            signature: String::new(),
            public_key: Vec::new(),
        }
    }
}

#[cfg(test)]
impl Default for UnsignedTransaction {
    fn default() -> Self {
        Self::ValueTransferTransaction {
            sender_addr: Address::default(),
            receiver_addr: Address::default(),
            info: TransactionInfo::default(),
        }
    }
}

#[cfg(test)]
impl Default for TransactionInfo {
    fn default() -> Self {
        TransactionInfo {
            amount: 1,
            asset_name: AssetName::BTC,
            timestamp: "2025-05-30T14:00:00Z".parse().unwrap(),
        }
    }
}

#[cfg(test)]
impl Default for Address {
    fn default() -> Self {
        Address(Vec::new())
    }
}
