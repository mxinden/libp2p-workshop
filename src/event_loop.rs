use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};
use libp2p::{
    core::ConnectedPoint, futures::StreamExt, identify, swarm::SwarmEvent, Multiaddr, PeerId, Swarm,
};
use std::{
    collections::{hash_map, HashMap},
    fmt::Debug,
};

#[derive(Debug)]
pub enum Command {
    Dial {
        peer_id: PeerId,
        sender: oneshot::Sender<Result<(), String>>,
    },
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Event {
    ConnectionEstablished { endpoint: ConnectedPoint },
    NewListenAddr { addr: Multiaddr },
    Identify { info: identify::Info, peer: PeerId },
}

pub struct EventLoop {
    swarm: Swarm<identify::Behaviour>,
    command_receiver: mpsc::UnboundedReceiver<Command>,
    event_sender: mpsc::UnboundedSender<Event>,
    pending_dial: HashMap<PeerId, oneshot::Sender<Result<(), String>>>,
}

impl EventLoop {
    pub fn new(
        swarm: Swarm<identify::Behaviour>,
        command_receiver: mpsc::UnboundedReceiver<Command>,
        event_sender: mpsc::UnboundedSender<Event>,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            pending_dial: Default::default(),
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

    async fn handle_event<E: Debug>(&mut self, event: SwarmEvent<identify::Event, E>) {
        match event {
            SwarmEvent::Behaviour(identify::Event::Received { peer_id, info }) => {
                let _ = self
                    .event_sender
                    .send(Event::Identify {
                        peer: peer_id,
                        info,
                    })
                    .await;
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
        }
    }
}
