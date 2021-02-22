use std::mem;
use std::sync::Arc;
use std::collections::HashMap;
use cid::Cid;
use futures::channel::{mpsc, oneshot};
use futures::{select, SinkExt};
use futures::StreamExt;

use libp2p_rs::core::PeerId;
use libp2p_rs::runtime::task;
use libp2p_rs::swarm::Control as SwarmControl;

use crate::block::Block;
use crate::control::Control;
use crate::error::BitswapError;
use crate::ledger::{Ledger, Message, Priority};
use crate::protocol::{Handler, ProtocolEvent, send_message};
use crate::stat::Stats;
use crate::BsBlockStore;
use libp2p_rs::swarm::protocol_handler::{ProtocolImpl, IProtocolHandler};

pub(crate) enum ControlCommand {
    WantBlock(Cid, oneshot::Sender<Result<Block>>),
    CancelBlock(Cid, oneshot::Sender<Result<()>>),
    WantList(Option<PeerId>, oneshot::Sender<Result<Vec<(Cid, Priority)>>>),
    Peers(oneshot::Sender<Result<Vec<PeerId>>>),
    Stats(oneshot::Sender<Result<Stats>>),
}

pub struct Bitswap<TBlockStore> {
    // Used to open stream.
    swarm: Option<SwarmControl>,

    /// block store
    blockstore: TBlockStore,

    // New peer is connected or peer is dead.
    peer_tx: mpsc::UnboundedSender<ProtocolEvent>,
    peer_rx: mpsc::UnboundedReceiver<ProtocolEvent>,

    // Used to recv incoming rpc message.
    incoming_tx: mpsc::UnboundedSender<(PeerId, Message)>,
    incoming_rx: mpsc::UnboundedReceiver<(PeerId, Message)>,

    // Used to pub/sub/ls/peers.
    control_tx: mpsc::UnboundedSender<ControlCommand>,
    control_rx: mpsc::UnboundedReceiver<ControlCommand>,

    /// Wanted blocks
    ///
    /// The oneshot::Sender is used to send the block back to the API users.
    wanted_blocks: HashMap<Cid, Vec<oneshot::Sender<Result<Block>>>>,

    /// Ledger
    connected_peers: HashMap<PeerId, Ledger>,

    /// Statistics related to peers.
    stats: HashMap<PeerId, Arc<Stats>>,
}

type Result<T> = std::result::Result<T, BitswapError>;

impl<TBlockStore: BsBlockStore> Bitswap<TBlockStore> {
    pub fn new(blockstore: TBlockStore) -> Self {
        let (peer_tx, peer_rx) = mpsc::unbounded();
        let (incoming_tx, incoming_rx) = mpsc::unbounded();
        let (control_tx, control_rx) = mpsc::unbounded();
        Bitswap {
            swarm: None,
            blockstore,
            peer_tx,
            peer_rx,
            incoming_tx,
            incoming_rx,
            control_tx,
            control_rx,
            wanted_blocks: Default::default(),
            connected_peers: Default::default(),
            stats: Default::default(),
        }
    }

    /// Get control of floodsub, which can be used to publish or subscribe.
    pub fn control(&self) -> Control {
        Control::new(self.control_tx.clone())
    }

    /// Message Process Loop.
    pub async fn process_loop(&mut self) -> Result<()> {
        loop {
            select! {
                cmd = self.peer_rx.next() => {
                    self.handle_event(cmd);
                }
                msg = self.incoming_rx.next() => {
                    if let Some((source, message)) = msg {
                        self.handle_incoming_message(source, message).await;
                    }
                }
                cmd = self.control_rx.next() => {
                    self.handle_control_command(cmd)?;
                }
            }
        }
    }

    fn send_message_to(&mut self, peer_id: PeerId, message: Message) {
        if let Some(peer_stats) = self.stats.get_mut(&peer_id) {
            peer_stats.update_outgoing(message.blocks.len() as u64);
        }

        // spwan a task to send the message
        let swarm = self.swarm.clone().expect("swarm??");
        task::spawn(async move {
            let _ = send_message(swarm, peer_id, message).await;
        });
    }
    //
    // async fn send_messages(&mut self) {
    //     for (peer_id, ledger) in &mut self.connected_peers {
    //         if let Some(message) = ledger.send() {
    //             if let Some(peer_stats) = self.stats.get_mut(peer_id) {
    //                 peer_stats.update_outgoing(message.blocks.len() as u64);
    //             }
    //
    //             // spwan a task to send the message
    //             let swarm = self.swarm.clone().expect("swarm??");
    //             let peer_id = *peer_id;
    //             task::spawn(async move {
    //                 let _ = send_message(swarm, peer_id, message).await;
    //             });
    //             // send meaasge
    //             // let _ = send_message(self.swarm.clone().unwrap(), peer_id.clone(), message).await;
    //         }
    //     }
    // }

    fn handle_event(&mut self, evt: Option<ProtocolEvent>) {
        match evt {
            Some(ProtocolEvent::Blocks(peer, blocks)) => {
                log::debug!("blockstore reports {} for {:?}", blocks.len(), peer);
                let ledger = self
                    .connected_peers
                    .get_mut(&peer)
                    .expect("Peer without ledger?!");
                //self.s
                blocks.into_iter().for_each(|block| ledger.add_block(block));

                if let Some(message) = ledger.send() {
                    self.send_message_to(peer, message);
                }
            }
            Some(ProtocolEvent::NewPeer(p)) => {
                log::debug!("{:?} connected", p);
                // make a ledge for the peer and send wantlist to it
                let ledger = Ledger::new();
                self.connected_peers.insert(p.clone(), ledger);
                self.stats.entry(p.clone()).or_default();
                self.send_want_list(p);
            }
            Some(ProtocolEvent::DeadPeer(p)) => {
                log::debug!("{:?} disconnected", p);
                self.connected_peers.remove(&p);
            }
            None => {}
        }
    }

    async fn handle_incoming_message(
        &mut self,
        source: PeerId,
        mut message: Message,
    ) {
        log::debug!("incoming message: from {}, w={} c={} b={}", source,
                    message.want().len(), message.cancel().len(), message.blocks().len());

        let current_wantlist = self.local_wantlist();

        let ledger = self
            .connected_peers
            .get_mut(&source)
            .expect("Peer without ledger?!");

        // Process the incoming cancel list.
        for cid in message.cancel() {
            ledger.received_want_list.remove(cid);
        }

        // Process the incoming wantlist.
        let mut to_get = vec![];
        for (cid, priority) in message
            .want()
            .iter()
            .filter(|&(cid, _)| !current_wantlist.contains(&cid))
        {
            ledger.received_want_list.insert(cid.to_owned(), *priority);
            to_get.push(cid.to_owned());
        }

        if to_get.len() > 0 {
            // ask blockstore for the wanted blocks
            log::debug!("{:?} asking for {} blocks", source, to_get.len());
            let blockstore = self.blockstore.clone();
            let mut poster = self.peer_tx.clone();
            task::spawn(async move {
                let mut blocks = vec![];
                for cid in to_get {
                    if let Ok(Some(block)) = blockstore.get(&cid).await {
                        //ledger.add_block(block);
                        blocks.push(block);
                    }
                }
                if blocks.len() > 0 {
                    let _ = poster.send(ProtocolEvent::Blocks(source, blocks)).await;
                }
            });
        }

        // Process the incoming blocks.
        // TODO: send block to any peer who want
        for block in mem::take(&mut message.blocks) {
            self.handle_received_block(source, block);
        }
    }

    fn handle_received_block(&mut self, source: PeerId, block: Block) {
        log::debug!("received {:?} from {:?}", block.cid, source);

        // publish block to all pending API users
        self.wanted_blocks.remove(&block.cid).map(|txs| {
            txs.into_iter().for_each(|tx| {
                // some tx may be dropped, regardless
                let _ = tx.send(Ok(block.clone()));
            })
        });

        // cancel want
        for (_peer_id, ledger) in self.connected_peers.iter_mut() {
            ledger.cancel_block(&block.cid);
        }

        // put block onto blockstore
        let blockstore = self.blockstore.clone();
        let peer_stats = Arc::clone(&self.stats.get(&source).unwrap());
        task::spawn(async move {
            let bytes = block.data().len() as u64;
            let res = blockstore.put(block.clone()).await;
            match res {
                Ok((_, true)) => {
                    peer_stats.update_incoming_unique(bytes);
                },
                Ok((_, false)) => {
                    peer_stats.update_incoming_duplicate(bytes);
                },
                Err(e) => {
                    log::info!(
                        "Got block {} from {:?} but failed to store it: {}",
                        block.cid,
                        source,
                        e
                    );
                }
            };
        });

    }

    fn handle_control_command(&mut self, cmd: Option<ControlCommand>) -> Result<()> {
        match cmd {
            Some(ControlCommand::WantBlock(cid, reply)) => {
                self.want_block(cid, 1, reply);
            }
            Some(ControlCommand::CancelBlock(cid, reply)) => {
                self.cancel_block(&cid, reply)
            },
            Some(ControlCommand::WantList(peer, reply)) => {
                if let Some(peer_id) = peer {
                    let list = self.peer_wantlist(&peer_id)
                        .unwrap_or_default();
                    let _ = reply.send(Ok(list));
                } else {
                    let list = self.local_wantlist()
                        .into_iter()
                        .map(|cid| (cid, 1))
                        .collect();
                    let _ = reply.send(Ok(list));
                }
            },
            Some(ControlCommand::Peers(reply)) => {
                let _ = reply.send(Ok(self.peers()));
            },
            Some(ControlCommand::Stats(reply)) => {
                let _ = reply.send(Ok(self.stats()));
            },
            None => {
                // control channel closed, exit the main loop
                return Err(BitswapError::Closing);
            }
        }
        Ok(())
    }

    /// Queues the wanted block for all peers.
    ///
    /// A user request
    pub fn want_block(&mut self, cid: Cid, priority: Priority, tx: oneshot::Sender<Result<Block>>) {
        log::debug!("bitswap want block {:?} ", cid);
        for (_peer_id, ledger) in self.connected_peers.iter_mut() {
            ledger.want_block(&cid, priority);
        }
        self.wanted_blocks.entry(cid).or_insert(vec![]).push(tx);
    }

    /// Removes the block from our want list and updates all peers.
    ///
    /// Can be either a user request or be called when the block
    /// was received.
    pub fn cancel_block(&mut self, cid: &Cid, tx: oneshot::Sender<Result<()>>) {
        log::debug!("bitswap cancel block {:?} ", cid);
        for (_peer_id, ledger) in self.connected_peers.iter_mut() {
            ledger.cancel_block(cid);
        }
        self.wanted_blocks.remove(cid);
        let _ = tx.send(Ok(()));
    }

    /// Returns the wantlist of a peer, if known
    pub fn peer_wantlist(&self, peer: &PeerId) -> Option<Vec<(Cid, Priority)>> {
        self.connected_peers.get(peer).map(Ledger::wantlist)
    }

    /// Returns the wantlist of the local node
    pub fn local_wantlist(&self) -> Vec<Cid> {
        self.wanted_blocks
            .iter()
            .map(|(cid, _)| cid.clone())
            .collect()
    }

    /// Returns the connected peers.
    pub fn peers(&self) -> Vec<PeerId> {
        self.connected_peers.keys().cloned().collect()
    }

    /// Returns the statistics of bitswap.
    pub fn stats(&self) -> Stats {
        self.stats
            .values()
            .fold(Stats::default(), |acc, peer_stats| {
                acc.add_assign(&peer_stats);
                acc
            })
    }

    /// Sends the wantlist to the peer.
    fn send_want_list(&mut self, peer_id: PeerId) {
        if !self.wanted_blocks.is_empty() {
            // FIXME: this can produce too long a message
            // FIXME: we should shard these across all of our peers by some logic; also, peers may
            // have been discovered to provide some specific wantlist item
            let mut message = Message::default();
            for (cid, _) in &self.wanted_blocks {
                // TODO: set priority
                message.want_block(cid, 1);
            }

            // spwan a task to send the message
            let swarm = self.swarm.clone().expect("swarm??");
            task::spawn(async move {
                let _ = send_message(swarm, peer_id, message).await;
            });
        }
    }
}

impl<TBlockStore: BsBlockStore> ProtocolImpl for Bitswap<TBlockStore> {

    /// Get handler of floodsub, swarm will call "handle" func after muxer negotiate success.
    fn handler(&self) -> IProtocolHandler {
        Box::new(Handler::new(self.incoming_tx.clone(), self.peer_tx.clone()))
    }

    /// Start message process loop.
    fn start(mut self, swarm: SwarmControl) -> Option<task::TaskHandle<()>> where
        Self: Sized, {
        self.swarm = Some(swarm);

        // well, self 'move' explicitly,
        let mut bitswap = self;

        Some(task::spawn(async move {
            log::info!("starting bitswap main loop...");
            let _ = bitswap.process_loop().await;
            log::info!("exiting bitswap main loop...");
        }))
    }
}