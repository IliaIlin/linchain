use chrono::Utc;
use k256::ecdsa::SigningKey;
use k256::elliptic_curve::rand_core::OsRng;
use libp2p::futures::StreamExt;
use libp2p::gossipsub::IdentTopic;
use libp2p::swarm::SwarmEvent;
use libp2p::{Swarm, mdns};
use peer::blockchain::{
    Address, PublicKey, SignedTransaction, TransactionInfo, UnsignedTransaction,
};
use peer::crypto::sign_ecdsa;
use peer::network;
use peer::network::{Message, NetworkEvent, P2PMdnsNetwork};
use peer::peer::AssetName;
use serde::Deserialize;
use std::collections::HashSet;
use tokio::time::interval;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let env_config = envy::from_env::<EnvConfig>()?;
    let mut swarm = setup_network(&env_config)?;
    let topic = IdentTopic::new(env_config.topic_name.clone());

    let mut timer = interval(std::time::Duration::from_secs(5));
    let mut dialing_peers = HashSet::new();

    let transaction = create_and_sign_topup_transaction(Address::from(vec![1, 2, 3, 4, 5]));

    loop {
        tokio::select! {
                    _ = timer.tick() => {
                        if let Err(e) = swarm.behaviour_mut().pub_sub.publish(
                            topic.clone(),
                            bcs::to_bytes(&Message::ClientTransaction(transaction.clone())).expect("Message is expected to be sent by client")
                        ){
                    eprintln!("Failed to publish message: {}", e);
                } else{
                    println!("Successful publish of the message");
                }
            }
                event = swarm.select_next_some() => {
                    match event {
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
    }
}

fn setup_network(
    env_config: &EnvConfig,
) -> Result<Swarm<P2PMdnsNetwork>, Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
    let mut swarm = network::build_p2p_network_swarm(env_config.stay_alive_secs)?;
    swarm.listen_on(env_config.network_address.clone())?;
    Ok(swarm)
}
fn create_and_sign_topup_transaction(address: Address) -> SignedTransaction {
    let transaction = UnsignedTransaction::TopUpTransaction {
        receiver_addr: address,
        info: TransactionInfo {
            amount: 5,
            asset_name: AssetName::ETH,
            timestamp: Utc::now(),
        },
    };
    let signing_key = SigningKey::random(&mut OsRng);
    let signature = sign_ecdsa(&transaction, &signing_key).unwrap();
    let public_key: PublicKey = signing_key.verifying_key().to_sec1_bytes().to_vec();
    SignedTransaction {
        transaction,
        signature,
        public_key,
    }
}

#[derive(Deserialize, Debug)]
struct EnvConfig {
    stay_alive_secs: u64,
    network_address: libp2p::Multiaddr,
    topic_name: String,
}
