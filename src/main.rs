use clap::Parser;
use futures::stream::StreamExt;
use libp2p::{identify, identity, swarm::SwarmEvent, Multiaddr, PeerId, Swarm};
use std::error::Error;

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
    let transport = libp2p::development_transport(local_key).await?;

    let mut swarm = Swarm::new(
        transport,
        identify::Behaviour::new(identify::Config::new(
            "/ipfs/0.1.0".into(),
            local_public_key.clone(),
        )),
        local_peer_id,
    );

    swarm.dial(opts.bootstrap_node)?;

    loop {
        match swarm.next().await.unwrap() {
            SwarmEvent::ConnectionEstablished { endpoint, .. } => {
                println!("Connected to {}.", endpoint.get_remote_address());
            }
            SwarmEvent::Behaviour(identify::Event::Received {
                peer_id: _,
                info: identify::Info { agent_version, .. },
            }) => {
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
