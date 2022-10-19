mod codec;
mod event_loop;

use async_std::io;

use clap::Parser;
use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
    select,
    stream::StreamExt,
};

use libp2p::{
    core, dns,
    gossipsub::{self},
    identify, identity,
    multiaddr::Protocol,
    noise, relay,
    request_response::{self},
    tcp, yamux, Multiaddr, NetworkBehaviour, PeerId, Swarm, Transport,
};

use event_loop::{Command, Event, EventLoop};
use std::{error::Error, iter, os::unix::prelude::FileExt, time::Duration};

#[allow(clippy::derive_partial_eq_without_eq)]
mod message_proto {
    include!(concat!(env!("OUT_DIR"), "/workshop.pb.rs"));
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

    // // Listen on a new address so that other peers can dial us.
    // //
    // // - IP 0.0.0.0 lets us listen on all network interfaces.
    // // - Port 0 uses a port assigned by the OS.
    // let local_address = "/ip4/0.0.0.0/tcp/0".parse().unwrap();
    // network.listen_on(local_address)?;

    // // Dial the bootstrap node.
    // network.dial(opts.bootstrap_node)?;

    network.listen_on(opts.bootstrap_node.clone().with(Protocol::P2pCircuit))?;

    // ----------------------------------------
    // Send and receive messages in the network.
    // ----------------------------------------
    let chat_topic = gossipsub::IdentTopic::new("chat");
    let addrs_topic = gossipsub::IdentTopic::new("addresses");
    let files_topic = gossipsub::IdentTopic::new("files");

    network.behaviour_mut().gossipsub.subscribe(&chat_topic)?;
    network.behaviour_mut().gossipsub.subscribe(&addrs_topic)?;
    network.behaviour_mut().gossipsub.subscribe(&files_topic)?;

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

    // ----------------------------------------
    // Run the network until we established a connection to the bootstrap node
    // and exchanged identify into
    // ----------------------------------------

    let (mut client, mut events_receiver) =
        Client::new(network, files_topic, chat_topic, addrs_topic);

    loop {
        select! {
            // Parse lines from Stdin
            line = stdin.select_next_some() => {

                let line = line.expect("Stdin not to close");

                let (prefix, arg) = match line.split_once(' ') {
                    Some(split) => split,
                    None => {
                        log::info!("Invalid command format");
                        continue;
                    }
                };
                match prefix {
                    "MSG" => match client.send_message(arg.to_string()).await {
                        Ok(()) => {}
                        Err(e) => log::info!("Publish error: {:?}", e),
                    }
                    "GET" => {
                        let file_name = arg.to_string();
                        let data = match client.request_file(file_name.clone()).await {
                            Ok(data) => data,
                            Err(err) => {
                                log::warn!("Error getting file {}", err);
                                continue
                            }
                        };
                        let file = match std::fs::File::create(file_name.clone()) {
                            Ok(file) => file,
                            Err(err) => {
                                log::warn!("Error creating file at {}: {:?}", file_name, err);
                                continue
                            }
                        };
                        match file.write_all_at(&data, 0) {
                            Ok(()) => log::info!("Downloaded new file: {:?}", file_name),
                            Err(err) => {
                                log::warn!("Error write to file at {}: {:?}", file_name, err)
                            }
                        }
                    }
                    "PUT" => {
                        let file_name = arg.to_string();
                        match client.start_providing(file_name.clone()).await {
                            Ok(()) => log::info!("Published {:?}", file_name),
                            Err(e) => log::warn!("Publishing file {} failed {}", file_name, e),
                        }
                    }
                    other => {
                        log::info!("Invalid prefix: Expected MSG|GET|PUT, found {}", other)
                    }
                }
            },

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
                Event::Identify( identify::Info { agent_version, .. }) => {
                    log::info!("Agent version {}", agent_version);
                }

                // Case 4: We learned about a file that another peer is providing.
                Event::NewProvider { peer, file} => {
                    log::info!("{:?} is now providing file {:?}", peer, file );
                }

                // Case 5: A remote peer published a message to the network
                Event::NewMessage {peer, message_id, message} => {
                    log::info!(
                        "Got message\n\tMessage Id: {}\n\tSender: {:?}\n\tMessage: {:?}",
                        message_id,
                        peer,
                        String::from_utf8_lossy(&message),
                    );
                }
            }
        }
    }
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

    // Gossipsub Protocol
    //
    // Publish-subscribe message protocol.
    let gossipsub_protocol = {
        // Set a custom gossipsub
        let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
            .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
            .build()
            .expect("Valid config");

        gossipsub::Gossipsub::new(
            gossipsub::MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
        .unwrap()
    };

    // Use a relay peer if we can not connect to another peer directly.
    let (relay_transport, relay_protocol) =
        relay::v2::client::Client::new_transport_and_behaviour(local_peer_id);

    let mut config = request_response::RequestResponseConfig::default();
    config.set_connection_keep_alive(Duration::from_secs(60));
    config.set_request_timeout(Duration::from_secs(60));

    // Enable direct 1:1 request-response messages.
    let direct_message_protocol = {
        let mut config = request_response::RequestResponseConfig::default();
        config.set_connection_keep_alive(Duration::from_secs(60));
        config.set_request_timeout(Duration::from_secs(60));
        request_response::RequestResponse::new(
            codec::Codec,
            iter::once((codec::Protocol, request_response::ProtocolSupport::Full)),
            config,
        )
    };

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
    let transport = relay_transport
        .or_transport(dns_tcp_transport)
        .upgrade(core::upgrade::Version::V1)
        .authenticate(noise::NoiseAuthenticated::xx(&local_key).unwrap())
        .multiplex(yamux::YamuxConfig::default())
        .timeout(std::time::Duration::from_secs(20))
        .boxed();

    Ok(Swarm::new(
        transport,
        Behaviour {
            identify: identify_protocol,
            gossipsub: gossipsub_protocol,
            relay: relay_protocol,
            request_response: direct_message_protocol,
        },
        local_peer_id,
    ))
}

#[derive(Clone)]
pub struct Client {
    sender: mpsc::Sender<Command>,
}

impl Client {
    pub fn new(
        network: Swarm<Behaviour>,
        files_topic: gossipsub::IdentTopic,
        chat_topic: gossipsub::IdentTopic,
        address_topic: gossipsub::IdentTopic,
    ) -> (Self, mpsc::Receiver<Event>) {
        let (event_tx, event_rx) = mpsc::channel(10);
        let (command_tx, command_rx) = mpsc::channel(10);
        async_std::task::spawn(
            EventLoop::new(
                network,
                command_rx,
                event_tx,
                files_topic,
                chat_topic,
                address_topic,
            )
            .run(),
        );
        (Client { sender: command_tx }, event_rx)
    }

    /// Listen for incoming connections on the given address.
    pub async fn start_listening(&mut self, addr: Multiaddr) {
        self.sender
            .send(Command::StartListening { addr })
            .await
            .unwrap();
    }

    /// Dial the given peer at the given address.
    pub async fn dial(&mut self, peer_id: PeerId) -> Result<(), String> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::Dial { peer_id, sender })
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    /// Advertise the local node as the provider of the given file on the DHT.
    pub async fn start_providing(&mut self, file_name: String) -> Result<(), String> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::Provide { file_name, sender })
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    /// Find the providers for the given file on the DHT.
    pub async fn send_message(&mut self, message: String) -> Result<(), String> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::Message { message, sender })
            .await
            .unwrap();
        receiver.await.unwrap()
    }

    /// Request the content of the given file from the given peer.
    pub async fn request_file(&mut self, file_name: String) -> Result<Vec<u8>, String> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::Get { file_name, sender })
            .await
            .unwrap();
        receiver.await.unwrap()
    }
}

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    identify: identify::Behaviour,
    gossipsub: gossipsub::Gossipsub,
    relay: relay::v2::client::Client,
    request_response: request_response::RequestResponse<codec::Codec>,
}

#[derive(Debug, Parser)]
#[clap(name = "libp2p-workshop-node")]
struct Opts {
    #[clap(long)]
    bootstrap_node: Multiaddr,
}
