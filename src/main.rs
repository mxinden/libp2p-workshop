use clap::Parser;
use futures::stream::StreamExt;
use libp2p::{
    gossipsub, identify, identity, swarm::SwarmEvent, Multiaddr, NetworkBehaviour, PeerId, Swarm,
};
use std::{
    error::Error,
    hash::{Hash, Hasher},
    time::Duration,
};

#[derive(Debug, Parser)]
#[clap(name = "libp2p-workshop-node")]
struct Opts {
    #[clap(long)]
    bootstrap_node: Multiaddr,
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let opts = Opts::parse();

    // Create a random PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_public_key = local_key.public();
    let local_peer_id = PeerId::from(local_public_key.clone());
    println!("Local peer id: {:?}", local_peer_id);

    // Set up an encrypted DNS-enabled TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::development_transport(local_key.clone()).await?;

    // To content-address message, we can take the hash of message and use it as an ID.
    let message_id_fn = |message: &gossipsub::GossipsubMessage| {
        let mut s = std::collections::hash_map::DefaultHasher::new();
        message.data.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_string())
    };

    // Set a custom gossipsub
    let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
        .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
        .message_id_fn(message_id_fn) // content-address messages. No two messages of the
        // same content will be propagated.
        .build()
        .expect("Valid config");

    let mut swarm = Swarm::new(
        transport,
        Behaviour {
            identify: identify::Behaviour::new(identify::Config::new(
                "/ipfs/0.1.0".into(),
                local_public_key.clone(),
            )),
            keep_alive: libp2p::swarm::keep_alive::Behaviour,
            gossipsub: libp2p::gossipsub::Gossipsub::new(
                gossipsub::MessageAuthenticity::Signed(local_key),
                gossipsub_config,
            )
            .unwrap(),
        },
        local_peer_id,
    );

    swarm.dial(opts.bootstrap_node)?;

    loop {
        match swarm.next().await.unwrap() {
            SwarmEvent::ConnectionEstablished { endpoint, .. } => {
                println!("Connected to {}.", endpoint.get_remote_address());
            }
            SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                peer_id: _,
                info: identify::Info { agent_version, .. },
            })) => {
                println!("Agent version {}", agent_version);
                break;
            }
            e => {
                log::debug!("{:?}", e)
            }
        }
    }

    Ok(())
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    keep_alive: libp2p::swarm::keep_alive::Behaviour,
    identify: libp2p::identify::Behaviour,
    gossipsub: libp2p::gossipsub::Gossipsub,
}
