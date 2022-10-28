use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};
use libp2p::{
    core::ConnectedPoint,
    futures::StreamExt,
    gossipsub::{GossipsubEvent, GossipsubMessage, IdentTopic, MessageId},
    identify,
    swarm::SwarmEvent,
    Multiaddr, PeerId, Swarm,
};
use std::{
    collections::{
        hash_map,
        HashMap,
    },
    fmt::Debug,
};

use crate::{Behaviour, BehaviourEvent};

#[derive(Debug)]
pub enum Command {
    Dial {
        peer_id: PeerId,
        sender: oneshot::Sender<Result<(), String>>,
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

    chat_topic: IdentTopic,
}

impl EventLoop {
    pub fn new(
        swarm: Swarm<Behaviour>,
        command_receiver: mpsc::UnboundedReceiver<Command>,
        event_sender: mpsc::UnboundedSender<Event>,
        chat_topic: IdentTopic,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            pending_dial: Default::default(),
            chat_topic,
        }
    }

    pub async fn run(mut self) {
        loop {
            futures::select! {
                event = self.swarm.next() => self.handle_event(event.expect("Swarm stream to be infinite.")).await  ,
                command = self.command_receiver.select_next_some() => self.handle_command(command).await,
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
