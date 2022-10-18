use prost::Message;
mod codec;
use async_std::io;
use asynchronous_codec::{Decoder, Encoder};
use clap::Parser;
use futures::{prelude::*, select, stream::StreamExt};
use libp2p::{
    core, dns,
    gossipsub::{self, GossipsubEvent, GossipsubMessage},
    identify, identity,
    multiaddr::Protocol,
    noise, relay,
    request_response::{self, RequestResponseEvent, RequestResponseMessage},
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, NetworkBehaviour, PeerId, Swarm, Transport,
};
use std::{
    collections::HashMap,
    error::Error,
    hash::{Hash, Hasher},
    io::Cursor,
    iter,
    os::unix::prelude::FileExt,
    time::Duration,
};

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

    // Listen on a new address so that other peers can dial us.
    //
    // - IP 0.0.0.0 lets us listen on all network interfaces.
    // - Port 0 uses a port assigned by the OS.
    let local_address = "/ip4/0.0.0.0/tcp/0".parse().unwrap();
    network.listen_on(local_address)?;

    network.listen_on(opts.bootstrap_node.clone().with(Protocol::P2pCircuit))?;

    // Dial the bootstrap node.
    network.dial(opts.bootstrap_node)?;

    // ----------------------------------------
    // Send and receive messages in the network.
    // ----------------------------------------
    let chat_topic = gossipsub::IdentTopic::new("chat");
    let addrs_topic = gossipsub::IdentTopic::new("addresses");
    let provider_topic = gossipsub::IdentTopic::new("files");

    network.behaviour_mut().gossipsub.subscribe(&chat_topic)?;
    network.behaviour_mut().gossipsub.subscribe(&addrs_topic)?;
    network
        .behaviour_mut()
        .gossipsub
        .subscribe(&provider_topic)?;

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

    let mut file_list = HashMap::new();
    let mut providing = HashMap::new();
    let mut pending_requests = HashMap::new();

    // ----------------------------------------
    // Run the network until we established a connection to the bootstrap node
    // and exchanged identify into
    // ----------------------------------------

    loop {
        select! {

            // Parse lines from Stdin
            line = stdin.select_next_some() => {

                let line = line.expect("Stdin not to close");

                let (prefix, arg) = match line.split_once(' ') {
                    Some(split) => split,
                    None => {
                        println!("Invalid command format");
                        continue;
                    }
                };
                match prefix {
                    "MSG" => {
                        if let Err(e) = network
                            .behaviour_mut()
                            .gossipsub
                            .publish(chat_topic.clone(), arg.as_bytes())
                        {
                            println!("Publish error: {:?}", e);
                        }
                    }
                    "GET" => {
                        let provider_id = match file_list.get(&arg.as_bytes().to_vec()) {
                            Some(provider_id) => provider_id,
                            None => {
                                println!("No provider known for: {:?}", arg);
                                continue;
                            }
                        };
                        let request_id = network.behaviour_mut().request_response.send_request(provider_id, arg.as_bytes().to_vec());
                        pending_requests.insert(request_id, arg.to_string());
                        println!("Requested file for: {:?}", arg);
                    }
                    "PUT" => {
                        if let Err(err) = std::fs::File::open(&arg) {
                            println!("Can not access file {:?}: {:?}", arg, err);
                            continue;
                        }
                        let file_name = arg.split('/').last().unwrap().to_string();

                        let announcement = message_proto::FileAnnouncement {
                            filename: file_name,
                            addrs: todo!(),
                        };
                        let mut encoded_msg = bytes::BytesMut::new();
                        announcement.encode(&mut encoded_msg)
                            .expect("BytesMut to have sufficient capacity.");
                        let mut dst = bytes::BytesMut::new();
                        unsigned_varint::codec::UviBytes::default()
                            .encode(encoded_msg.freeze(), &mut dst)?;

                        match network
                            .behaviour_mut()
                            .gossipsub
                            .publish(provider_topic.clone(), dst)
                        {
                            Ok(_) => {
                                println!("Published file {:?}", file_name);
                                providing.insert(file_name.to_string(), arg.to_string());
                            },
                            Err(e) => println!("Publish error: {:?}", e),
                        }
                    }
                    other => {
                        println!("Invalid prefix: Expected MSG|GET|PUT, found {}", other)
                    }
                }
            },


            // Wait for an event happening on the network.
            // The `match` statement allows to match on the type
            // of event an handle each event differently.
            event = network.select_next_some() => match event {

                // Case 1: We are now actively listening on an address
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {}.", address);

                    if let Err(e) = network
                        .behaviour_mut()
                        .gossipsub
                        .publish(addrs_topic.clone(), address.to_vec())
                    {
                        println!("Publish error: {:?}", e);
                    }
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
                }

                // CWe received a message from another peer.
                SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(GossipsubEvent::Message {
                    message_id,
                    message: GossipsubMessage { topic, data, source, ..},
                    ..
                })) => {
                    if topic == chat_topic.hash() {
                        println!(
                            "Got message\n\tMessage Id: {}\n\tSender: {:?}\n\tMessage: {:?}",
                            message_id,
                            source.unwrap(),
                            String::from_utf8_lossy(&data),
                        );
                    } else if topic == provider_topic.hash(){
                        let mut b: bytes::BytesMut = data.as_slice().into();
                        let mut uvi: unsigned_varint::codec::UviBytes  = unsigned_varint::codec::UviBytes::default();
                        let file_announcement = uvi.decode(&mut b)?
                            .map(|msg| message_proto::FileAnnouncement::decode(Cursor::new(msg)));

                        file_list.insert(data, source.unwrap());

                    } else if topic == addrs_topic.hash() {
                        let addr = Multiaddr::try_from(data).unwrap();
                        network.behaviour_mut().request_response.add_address(&source.unwrap(), addr)
                    }
                }

                SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                    RequestResponseEvent::Message { message, .. },
                )) => match message {
                    RequestResponseMessage::Request {
                        request, channel, ..
                    } => {
                        let file_content = match String::from_utf8(request.clone()).ok().and_then(|file_name| providing.get(&file_name))
                        .and_then(|file_path|std::fs::read(&file_path).ok()) {
                            Some(path) => path,
                            None => {
                                println!("Got request for invalid file path: {:?}", request);
                                continue;
                            }
                        };
                        let _ = network.behaviour_mut().request_response.send_response(channel, file_content);
                    }
                    RequestResponseMessage::Response {
                        request_id,
                        response,
                    } => {
                        let file_name = pending_requests.remove(&request_id).unwrap();
                        let file = match std::fs::File::create(file_name.clone()) {
                            Ok(file) => file,
                            Err(err) => {
                                println!("Error creating file at {}: {:?}", file_name, err);
                                continue
                            }
                        };
                        match file.write_all_at(&response, 0) {
                            Ok(()) => println!("Downloaded new file: {:?}", file_name),
                            Err(err) => {
                                println!("Error write to file at {}: {:?}", file_name, err)
                            }
                        }
                    }
                },

                _ => {}
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

    println!("Local peer id: {:?}", local_peer_id);

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
            gossipsub::MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
        .unwrap()
    };

    // Use a relay peer if we can not connect to another peer directly.
    let (relay_transport, relay_protocol) =
        relay::v2::client::Client::new_transport_and_behaviour(local_peer_id);

    // Enable direct 1:1 request-response messages.
    let direct_message_protocol = request_response::RequestResponse::new(
        codec::Codec,
        iter::once((codec::Protocol, request_response::ProtocolSupport::Full)),
        request_response::RequestResponseConfig::default(),
    );

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

#[derive(NetworkBehaviour)]
struct Behaviour {
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
