use k256::ecdsa::SigningKey;
use k256::elliptic_curve::rand_core::OsRng;
use libp2p::Swarm;
use libp2p::gossipsub::IdentTopic;
use peer::network;
use peer::network::P2PMdnsNetwork;
use peer::peer::{LeaderElection, Peer, State};
use peer::storage::FileStorage;
use serde_derive::Deserialize;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let env_config = envy::from_env::<EnvConfig>()?;
    let swarm = setup_network(&env_config)?;
    create_peer_and_run(&env_config, swarm).await
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

async fn create_peer_and_run(
    env_config: &EnvConfig,
    swarm: Swarm<P2PMdnsNetwork>,
) -> Result<(), Box<dyn std::error::Error>> {
    let topic = IdentTopic::new(env_config.topic_name.to_string());

    let leader_election = LeaderElection::new();
    let mut peer = Peer::new(env_config.block_size);
    let storage = FileStorage {
        filename: env_config.storage_filename.to_string(),
    };
    let mut initial_state = State::initialize(storage.load_all_blocks_if_file_exists()?)
        .expect("Failed to initialize state");
    let signing_key = SigningKey::random(&mut OsRng);
    let signing_key_redacted = redact::Secret::new(&signing_key);
    peer.run(
        signing_key_redacted,
        &mut initial_state,
        &storage,
        swarm,
        topic,
        leader_election,
    )
    .await;
    Ok(())
}

#[derive(Deserialize, Debug)]
struct EnvConfig {
    stay_alive_secs: u64,
    network_address: libp2p::Multiaddr,
    topic_name: String,
    storage_filename: String,
    block_size: usize,
}
