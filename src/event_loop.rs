use asynchronous_codec::{Decoder, Encoder};
use futures::{
    channel::{mpsc, oneshot},
    FutureExt, SinkExt,
};
use futures_timer::Delay;
use libp2p::{
    core::ConnectedPoint,
    futures::StreamExt,
    gossipsub::{GossipsubEvent, GossipsubMessage, IdentTopic, MessageId},
    identify,
    mdns::MdnsEvent,
    request_response::RequestId,
    request_response::{RequestResponseEvent, RequestResponseMessage},
    swarm::SwarmEvent,
    Multiaddr, PeerId, Swarm,
};
use prost::Message;
use std::{
    collections::{
        hash_map::{self, Entry},
        HashMap, HashSet,
    },
    fmt::Debug,
    io::Cursor,
    time::Duration,
};

use crate::{message_proto, Behaviour, BehaviourEvent};

#[derive(Debug)]
pub enum Command {
    Dial {
        peer_id: PeerId,
        sender: oneshot::Sender<Result<(), String>>,
    },
    Provide {
        file_name: String,
        sender: oneshot::Sender<Result<(), String>>,
    },
    Get {
        file_name: String,
        sender: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    Message {
        message: String,
        sender: oneshot::Sender<Result<(), String>>,
    },
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Event {
    ConnectionEstablished {
        endpoint: ConnectedPoint,
    },
    NewListenAddr {
        address: Multiaddr,
    },
    Identify {
        info: identify::Info,
        peer: PeerId,
    },
    NewProvider {
        peer: PeerId,
        file: String,
    },
    NewMessage {
        peer: PeerId,
        message_id: MessageId,
        message: Vec<u8>,
    },
}

pub struct EventLoop {
    swarm: Swarm<Behaviour>,
    command_receiver: mpsc::UnboundedReceiver<Command>,
    event_sender: mpsc::UnboundedSender<Event>,
    pending_dial: HashMap<PeerId, oneshot::Sender<Result<(), String>>>,

    files_topic: IdentTopic,
    chat_topic: IdentTopic,

    known_files: HashMap<String, PeerId>,
    provided_files: HashMap<String, String>,
    pending_requests: HashMap<RequestId, oneshot::Sender<Result<Vec<u8>, String>>>,
    known_peers: HashSet<PeerId>,
}

impl EventLoop {
    pub fn new(
        swarm: Swarm<Behaviour>,
        command_receiver: mpsc::UnboundedReceiver<Command>,
        event_sender: mpsc::UnboundedSender<Event>,
        files_topic: IdentTopic,
        chat_topic: IdentTopic,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            pending_dial: Default::default(),
            known_files: HashMap::new(),
            provided_files: HashMap::new(),
            pending_requests: HashMap::new(),
            files_topic,
            chat_topic,
            known_peers: HashSet::new(),
        }
    }

    pub async fn run(mut self) {
        let mut republish_delay = Delay::new(Duration::from_secs(5)).fuse();
        loop {
            futures::select! {
                event = self.swarm.next() => self.handle_event(event.expect("Swarm stream to be infinite.")).await  ,
                command = self.command_receiver.select_next_some() => self.handle_command(command).await,
                _ = republish_delay => {
                    self.republish_file();
                    republish_delay = Delay::new(Duration::from_secs(5)).fuse();
                }
            }
        }
    }

    fn republish_file(&mut self) {
        for filename in self.provided_files.keys() {
            let listen_addrs = self.swarm.listeners().map(|a| a.to_vec()).collect();

            let announcement = message_proto::FileAnnouncement {
                filename: filename.clone(),
                addrs: listen_addrs,
            };

            let mut encoded_msg = bytes::BytesMut::new();
            announcement.encode(&mut encoded_msg).unwrap();
            let mut dst = bytes::BytesMut::new();
            unsigned_varint::codec::UviBytes::default()
                .encode(encoded_msg.freeze(), &mut dst)
                .unwrap();

            match self
                .swarm
                .behaviour_mut()
                .gossipsub
                .publish(self.files_topic.clone(), dst)
            {
                Ok(_) => {
                    log::debug!("Published file {:?}", filename);
                }
                Err(e) => log::warn!("Publish error: {:?}", e),
            }
        }
    }

    async fn handle_event<E: Debug>(&mut self, event: SwarmEvent<BehaviourEvent, E>) {
        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                peer_id,
                info,
            })) => {
                let _ = self
                    .event_sender
                    .send(Event::Identify {
                        peer: peer_id,
                        info,
                    })
                    .await;
            }
            SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                RequestResponseEvent::Message { message, .. },
            )) => match message {
                RequestResponseMessage::Request {
                    request, channel, ..
                } => {
                    let file_content = match String::from_utf8(request.clone())
                        .ok()
                        .and_then(|file_name| self.provided_files.get(&file_name))
                        .and_then(|file_path| std::fs::read(&file_path).ok())
                    {
                        Some(path) => path,
                        None => {
                            log::debug!("Got request for invalid file path: {:?}", request);
                            return;
                        }
                    };
                    let _ = self
                        .swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, file_content);
                }
                RequestResponseMessage::Response {
                    request_id,
                    response,
                } => {
                    let _ = self
                        .pending_requests
                        .remove(&request_id)
                        .expect("Request to still be pending.")
                        .send(Ok(response));
                }
            },
            SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                RequestResponseEvent::OutboundFailure {
                    request_id, error, ..
                },
            )) => {
                let _ = self
                    .pending_requests
                    .remove(&request_id)
                    .expect("Request to still be pending.")
                    .send(Err(error.to_string()));
            }
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(GossipsubEvent::Message {
                message_id,
                message:
                    GossipsubMessage {
                        topic,
                        data,
                        source,
                        ..
                    },
                ..
            })) => {
                let source = source.unwrap();
                if topic == self.chat_topic.hash() {
                    let _ = self
                        .event_sender
                        .send(Event::NewMessage {
                            peer: source,
                            message_id,
                            message: data,
                        })
                        .await;
                } else if topic == self.files_topic.hash() {
                    let mut b: bytes::BytesMut = data.as_slice().into();
                    let mut uvi: unsigned_varint::codec::UviBytes =
                        unsigned_varint::codec::UviBytes::default();
                    let file_announcement = match uvi.decode(&mut b).unwrap().and_then(|msg| {
                        message_proto::FileAnnouncement::decode(Cursor::new(msg)).ok()
                    }) {
                        Some(decoded) => decoded,
                        None => {
                            log::debug!("Received invalid message: {:?}", data);
                            return;
                        }
                    };
                    for addr in file_announcement.addrs {
                        self.swarm
                            .behaviour_mut()
                            .request_response
                            .add_address(&source, Multiaddr::try_from(addr).unwrap());
                    }
                    if let Entry::Vacant(e) =
                        self.known_files.entry(file_announcement.filename.clone())
                    {
                        e.insert(source);
                        let _ = self
                            .event_sender
                            .send(Event::NewProvider {
                                peer: source,
                                file: file_announcement.filename,
                            })
                            .await;
                    }
                }
            }
            SwarmEvent::ConnectionEstablished {
                peer_id: _,
                endpoint,
                ..
            } => {
                let _ = self
                    .event_sender
                    .send(Event::ConnectionEstablished { endpoint })
                    .await;
            }
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(MdnsEvent::Discovered(list))) => {
                for (peer, addr) in list {
                    self.swarm
                        .behaviour_mut()
                        .request_response
                        .add_address(&peer, addr);
                    if self.known_peers.insert(peer) {
                        let _ = self.swarm.dial(peer);
                    }
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                let _ = self
                    .event_sender
                    .send(Event::NewListenAddr { address })
                    .await;
            }
            event => log::debug!("{:?}", event),
        }
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::Dial { peer_id, sender } => {
                if let hash_map::Entry::Vacant(e) = self.pending_dial.entry(peer_id) {
                    match self.swarm.dial(peer_id) {
                        Ok(()) => {
                            e.insert(sender);
                        }
                        Err(e) => {
                            let _ = sender.send(Err(e.to_string()));
                        }
                    }
                } else {
                    log::debug!("Already dialing peer.");
                }
            }
            Command::Provide { file_name, sender } => {
                let path = std::path::Path::new(&file_name);
                let ret = match std::fs::File::open(&file_name) {
                    Ok(_) => {
                        let key = path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_owned())
                            .unwrap();
                        self.provided_files.insert(key, file_name);
                        Ok(())
                    }
                    Err(_e) => Err(format!("Could not open file {}", file_name)),
                };
                let _ = sender.send(ret);
            }
            Command::Get { file_name, sender } => {
                let provider_id = match self.known_files.get(&file_name) {
                    Some(provider_id) => provider_id,
                    None => {
                        let _ = sender.send(Err(format!("No provider known for: {:?}", file_name)));
                        return;
                    }
                };
                let request_id = self
                    .swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(provider_id, file_name.as_bytes().to_vec());
                self.pending_requests.insert(request_id, sender);
            }
            Command::Message { message, sender } => {
                let ret = match self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(self.chat_topic.clone(), message)
                {
                    Ok(_) => Ok(()),
                    Err(e) => Err(e.to_string()),
                };
                let _ = sender.send(ret);
            }
        }
    }
}
