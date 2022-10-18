use async_std::io;
use clap::Parser;
use futures::{prelude::*, select, stream::StreamExt};
use libp2p::{
    core::{self, muxing::StreamMuxerBox, transport::Boxed},
    dns,
    gossipsub::{self, GossipsubEvent},
    identify, identity, noise,
    relay::v2::relay,
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, NetworkBehaviour, PeerId, Swarm, Transport,
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

// Create a new network node.
async fn create_network() -> Result<Swarm<Behaviour>, Box<dyn Error>> {
    // ----------------------------------------
    // # Generate a new identity
    // ----------------------------------------

    // Create a random keypair that is used to authenticate ourself in the network.
    let local_key = identity::Keypair::generate_ed25519();
    let local_public_key = local_key.public();

    // Derive our PeerId from the public key.
    // The PeerId servers as a unique identifier in the network.
    let local_peer_id = PeerId::from(local_public_key.clone());

    println!("Local peer id: {:?}", local_peer_id);

    // ----------------------------------------
    // # Create our transport layer
    // ----------------------------------------

    // Use TCP as transport protocol.
    let tcp_transport = tcp::TcpTransport::new(tcp::GenTcpConfig::new().nodelay(true));

    // Enable DNS name resolution.
    let dns_tcp_transport = dns::DnsConfig::system(tcp_transport).await?;

    // Upgrade our transport:
    //
    // - Noise security: Authenticates peers and encrypts all traffic
    // - Yamux multiplexing: Abstracts a single connection into multiple logical streams
    //   that can be used by different application protocols.
    let transport = dns_tcp_transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(noise::NoiseAuthenticated::xx(&local_key).unwrap())
        .multiplex(yamux::YamuxConfig::default())
        .timeout(std::time::Duration::from_secs(20))
        .boxed();

    // ----------------------------------------
    // # Define our application layer protocols
    // ----------------------------------------

    // Identify Protocol
    //
    // Exchanges identify info with other peers.
    // In this info we inform the remote of e.g. our public key, local addresses, and version.
    // We also inform the remote at which address we observe them. This is important for the remote
    // since their public IP may differ from local listening address.
    let identify_protocol = identify::Behaviour::new(identify::Config::new(
        "/libp2p-workshop/0.1.0".into(),
        local_public_key.clone(),
    ));

    // Gossipsub Protocol
    //
    // Publish-subscribe message protocol.
    let gossipsub_protocol = {
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

        gossipsub::Gossipsub::new(
            gossipsub::MessageAuthenticity::Signed(local_key),
            gossipsub_config,
        )
        .unwrap()
    };

    Ok(Swarm::new(
        transport,
        Behaviour {
            identify: identify_protocol,
            gossipsub: gossipsub_protocol,
        },
        local_peer_id,
    ))
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let opts = Opts::parse();

    // Configure a new network.
    let mut network = create_network().await?;

    // ----------------------------------------
    // # Joining the network
    // ----------------------------------------

    // Listen on a new address so that other peers can dial us.
    //
    // - IP 0.0.0.0 lets us listen on all network interfaces.
    // - Port 0 uses a port assigned by the OS.
    let local_address = "/ip4/0.0.0.0/tcp/0".parse().unwrap();
    network.listen_on(local_address)?;

    // Dial the bootstrap node.
    network.dial(opts.bootstrap_node)?;

    // ----------------------------------------
    // Run the network until we established a connection to the bootstrap node
    // and exchanged identify into
    // ----------------------------------------

    loop {
        // Wait for an event happening on the network.
        // The `match` statement allows to match on the type
        // of event an handle each event differently.
        match network.next().await.unwrap() {
            // Case 1: We are now actively listening on an address
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {}.", address);
            }

            // Case 2: A connection to another peer was established
            SwarmEvent::ConnectionEstablished { endpoint, .. } => {
                println!("Connected to {}.", endpoint.get_remote_address());
            }

            // Case 3: A remote send us their identify info with the identify protocol.
            SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                peer_id: _,
                info: identify::Info { agent_version, .. },
            })) => {
                println!("Agent version {}", agent_version);
                break;
            }

            // Any other event happened
            e => {
                log::debug!("{:?}", e)
            }
        }
    }

    // ----------------------------------------
    // Send and receive messages in the network.
    // ----------------------------------------
    let topic = gossipsub::IdentTopic::new("chat");

    network.behaviour_mut().gossipsub.subscribe(&topic)?;

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();
    loop {
        select! {

            // Parse lines from Stdin
            line = stdin.select_next_some() => {
                if let Err(e) = network
                    .behaviour_mut()
                    .gossipsub
                    .publish(topic.clone(), line.expect("Stdin not to close").as_bytes())
                {
                    println!("Publish error: {:?}", e);
                }
            },

            // Handle events happening in the network.
            event = network.select_next_some() => match event {

                // CWe received a message from another peer.
                SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(GossipsubEvent::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                })) => println!(
                    "Got message\n\tMessage Id: {}\n\tSender: {:?}\n\tMessage: {:?}",
                    id,
                    peer_id,
                    String::from_utf8_lossy(&message.data),
                ),
                _ => {}
            }
        }
    }

    Ok(())
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    identify: identify::Behaviour,
    gossipsub: gossipsub::Gossipsub,
}
