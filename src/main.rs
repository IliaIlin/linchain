use k256::ecdsa::SigningKey;
use k256::elliptic_curve::rand_core::OsRng;
use libp2p::gossipsub::IdentTopic;
use linchain::network::build_p2p_network_swarm;
use linchain::peer::{Peer, State};
use linchain::storage::FileStorage;
use tracing_subscriber::EnvFilter;

// read those props from config or from command line args?
const LISTENING_NETWORK_ADDRESS: &str = "/ip4/0.0.0.0/tcp/0";
const TOPIC_NAME: &str = "linchain_topic";
const STAY_ALIVE_SECS: u64 = 120;
const BLOCK_SIZE: usize = 2;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let mut swarm = build_p2p_network_swarm(STAY_ALIVE_SECS).expect("Failed to build swarm");
    swarm.listen_on(LISTENING_NETWORK_ADDRESS.parse().expect(&format!(
        "Fatal: address {} can't be listened on",
        LISTENING_NETWORK_ADDRESS
    )))?;
    let topic = IdentTopic::new(TOPIC_NAME);

    let mut peer = Peer::new(BLOCK_SIZE);
    let storage = FileStorage {
        filename: "1.jsonl".to_string(),
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
    )
    .await;
    Ok(())
}
