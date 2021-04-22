use async_trait::async_trait;
use futures::channel::mpsc;
use futures::SinkExt;
use std::error::Error;

use libp2p_rs::core::upgrade::UpgradeInfo;
use libp2p_rs::core::{PeerId, ProtocolId};
use libp2p_rs::swarm::connection::Connection;
use libp2p_rs::swarm::protocol_handler::{IProtocolHandler, Notifiee, ProtocolHandler};
use libp2p_rs::swarm::substream::Substream;
use libp2p_rs::swarm::Control as SwarmControl;
use libp2p_rs::core::{ReadEx, WriteEx};

use crate::ledger::Message;
use crate::{Block, BS_PROTO_ID};

const MAX_BUF_SIZE: usize = 524_288;

pub(crate) enum ProtocolEvent {
    NewPeer(PeerId),
    DeadPeer(PeerId),
    Blocks(PeerId, Vec<Block>),
}

#[derive(Clone)]
pub struct Handler {
    incoming_tx: mpsc::UnboundedSender<(PeerId, Message)>,
    new_peer: mpsc::UnboundedSender<ProtocolEvent>,
}

impl Handler {
    pub(crate) fn new(
        incoming_tx: mpsc::UnboundedSender<(PeerId, Message)>,
        new_peer: mpsc::UnboundedSender<ProtocolEvent>,
    ) -> Self {
        Handler {
            incoming_tx,
            new_peer,
        }
    }
}

impl UpgradeInfo for Handler {
    type Info = ProtocolId;

    fn protocol_info(&self) -> Vec<Self::Info> {
        vec![BS_PROTO_ID.into()]
    }
}

impl Notifiee for Handler {
    fn connected(&mut self, conn: &mut Connection) {
        let peer_id = conn.remote_peer();
        let new_peers = self.new_peer.clone();
        let _ = new_peers.unbounded_send(ProtocolEvent::NewPeer(peer_id));
    }
    fn disconnected(&mut self, conn: &mut Connection) {
        let peer_id = conn.remote_peer();
        let new_peers = self.new_peer.clone();
        let _ = new_peers.unbounded_send(ProtocolEvent::DeadPeer(peer_id));
    }
}

#[async_trait]
impl ProtocolHandler for Handler {
    async fn handle(
        &mut self,
        mut stream: Substream,
        _info: <Self as UpgradeInfo>::Info,
    ) -> Result<(), Box<dyn Error>> {
        log::trace!("Handle stream from {}", stream.remote_peer());
        loop {
            let packet = stream.read_one(MAX_BUF_SIZE).await?;
            let message = Message::from_bytes(&packet)?;
            let peer = stream.remote_peer();
            self.incoming_tx.send((peer, message)).await?;
        }
    }

    fn box_clone(&self) -> IProtocolHandler {
        Box::new(self.clone())
    }
}

// Sends bitswap message to remote peer.
pub(crate) async fn send_message(
    mut swarm: SwarmControl,
    peer_id: PeerId,
    message: Message,
) -> Result<(), Box<dyn Error>> {
    log::debug!("sending message to {:?}...", peer_id);
    let mut stream = swarm.new_stream(peer_id, vec![BS_PROTO_ID.into()]).await?;
    stream.write_one(message.to_bytes().as_ref()).await?;
    Ok(())
}
