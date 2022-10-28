mod event_loop;

use clap::Parser;
use env_logger::Env;
use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
    select,
    stream::StreamExt,
};
use libp2p::{
    core, dns, identify, identity, noise, tcp, yamux, Multiaddr, PeerId, Swarm, Transport,
};
use std::error::Error;

use event_loop::{Command, Event, EventLoop};

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let opts = Opts::parse();

    // Configure a new network.
    let mut swarm = create_network().await?;

    // ----------------------------------------
    // # Joining the network
    // ----------------------------------------

    // Listen on a new address so that other peers can dial us.
    //
    // - IP 0.0.0.0 lets us listen on all network interfaces.
    // - Port 0 uses a port assigned by the OS.
    let local_address = "/ip4/0.0.0.0/tcp/0".parse().unwrap();
    swarm.listen_on(local_address)?;

    // Dial the bootstrap node.
    swarm.dial(opts.bootstrap_node)?;

    // ----------------------------------------
    // Run the network until we established a connection to the bootstrap node
    // and exchanged identify into
    // ----------------------------------------

    let (mut _network, mut events_receiver) = Network::new(swarm);

    loop {
        select! {
            // Wait for an event happening on the network.
            // The `match` statement allows to match on the type
            // of event an handle each event differently.
            event = events_receiver.select_next_some() => match event {
                // Case 1: We are now actively listening on an address
                Event::NewListenAddr { addr } => {
                    log::info!("Listening on {}.", addr);
                }

                // Case 2: A connection to another peer was established
                Event::ConnectionEstablished { endpoint } => {
                    log::info!("Connected to {}.", endpoint.get_remote_address());
                }

                // Case 3: A remote send us their identify info with the identify protocol.
                Event::Identify { peer, info: identify::Info { agent_version, .. }} => {
                    log::info!("Received Identify Info\nPeer: {}, Agent version {}", peer, agent_version);
                }
            }
        }
    }
}

// Create a new network node.
async fn create_network() -> Result<Swarm<identify::Behaviour>, Box<dyn Error>> {
    // ----------------------------------------
    // # Generate a new identity
    // ----------------------------------------

    // Create a random keypair that is used to authenticate ourself in the network.
    let local_key = identity::Keypair::generate_ed25519();
    let local_public_key = local_key.public();

    // Derive our PeerId from the public key.
    // The PeerId servers as a unique identifier in the network.
    let local_peer_id = PeerId::from(local_public_key.clone());

    log::info!("Local peer id: {:?}", local_peer_id);

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

    Ok(Swarm::new(transport, identify_protocol, local_peer_id))
}

#[derive(Clone)]
pub struct Network {
    sender: mpsc::UnboundedSender<Command>,
}

impl Network {
    pub fn new(network: Swarm<identify::Behaviour>) -> (Self, mpsc::UnboundedReceiver<Event>) {
        let (event_tx, event_rx) = mpsc::unbounded();
        let (command_tx, command_rx) = mpsc::unbounded();
        async_std::task::spawn(EventLoop::new(network, command_rx, event_tx).run());
        (Network { sender: command_tx }, event_rx)
    }

    /// Dial the given peer if we know their address.
    pub async fn dial(&mut self, peer_id: PeerId) -> Result<(), String> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::Dial { peer_id, sender })
            .await
            .unwrap();
        receiver.await.unwrap()
    }
}

#[derive(Debug, Parser)]
#[clap(name = "libp2p-workshop-node")]
struct Opts {
    #[clap(long)]
    bootstrap_node: Multiaddr,
}
