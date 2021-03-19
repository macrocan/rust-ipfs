use std::time::Duration;

use crate::p2p::MultiaddrWithPeerId;

/// A description of currently active connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// The connected peer along with its address.
    pub addr: MultiaddrWithPeerId,
    /// Latest ping report on any of the connections.
    pub rtt: Option<Duration>,
}

// /// Disconnected will use banning to disconnect a node. Disconnecting a single peer connection is
// /// not supported at the moment.
// pub struct Disconnector {
//     peer_id: PeerId,
// }
//
// impl Disconnector {
//     pub fn disconnect<T: NetworkBehaviour>(self, swarm: &mut Swarm<T>) {
//         Swarm::ban_peer_id(swarm, self.peer_id);
//         Swarm::unban_peer_id(swarm, self.peer_id);
//     }
// }
//
// #[derive(Debug, Default)]
// pub struct SwarmApi {
//     events: VecDeque<NetworkBehaviourAction>,
//     peers: HashSet<PeerId>,
//     connect_registry: SubscriptionRegistry<(), String>,
//     connections: HashMap<MultiaddrWithoutPeerId, PeerId>,
//     roundtrip_times: HashMap<PeerId, Duration>,
//     connected_peers: HashMap<PeerId, Vec<MultiaddrWithoutPeerId>>,
//     pub(crate) bootstrappers: HashSet<MultiaddrWithPeerId>,
// }
//
// impl SwarmApi {
//     pub fn add_peer(&mut self, peer_id: PeerId) {
//         self.peers.insert(peer_id);
//     }
//
//     pub fn peers(&self) -> impl Iterator<Item = &PeerId> {
//         self.peers.iter()
//     }
//
//     pub fn remove_peer(&mut self, peer_id: &PeerId) {
//         self.peers.remove(peer_id);
//     }
//
//     pub fn connections(&self) -> impl Iterator<Item = Connection> + '_ {
//         self.connected_peers
//             .iter()
//             .filter_map(move |(peer, conns)| {
//                 let rtt = self.roundtrip_times.get(peer).cloned();
//
//                 if let Some(any) = conns.first() {
//                     Some(Connection {
//                         addr: MultiaddrWithPeerId::from((any.clone(), *peer)),
//                         rtt,
//                     })
//                 } else {
//                     None
//                 }
//             })
//     }
//
//     pub fn set_rtt(&mut self, peer_id: &PeerId, rtt: Duration) {
//         // FIXME: this is for any connection
//         self.roundtrip_times.insert(*peer_id, rtt);
//     }
//
//     pub fn connect(&mut self, addr: MultiaddrWithPeerId) -> Option<SubscriptionFuture<(), String>> {
//         if self.connections.contains_key(&addr.multiaddr) {
//             return None;
//         }
//
//         trace!("Connecting to {:?}", addr);
//
//         let subscription = self
//             .connect_registry
//             .create_subscription(addr.clone().into(), None);
//
//         // libp2p currently doesn't support dialing with the P2p protocol, so only consider the
//         // "bare" Multiaddr
//         let MultiaddrWithPeerId { multiaddr, .. } = addr;
//
//         self.events.push_back(NetworkBehaviourAction::DialAddress {
//             address: multiaddr.into(),
//         });
//
//         Some(subscription)
//     }
//
//     pub fn disconnect(&mut self, addr: MultiaddrWithPeerId) -> Option<Disconnector> {
//         trace!("disconnect {}", addr);
//         // FIXME: closing a single specific connection would be allowed for ProtocolHandlers
//         if let Some(peer_id) = self.connections.remove(&addr.multiaddr) {
//             // wasted some time wondering if the peer should be removed here or not; it should. the
//             // API is a bit ackward since we can't tolerate the Disconnector::disconnect **not**
//             // being called.
//             //
//             // there are currently no events being fired from the closing of connections to banned
//             // peer, so we need to modify the accounting even before the banning happens.
//             self.mark_disconnected(&peer_id);
//             Some(Disconnector { peer_id })
//         } else {
//             None
//         }
//     }
//
//     fn mark_disconnected(&mut self, peer_id: &PeerId) {
//         for address in self.connected_peers.remove(peer_id).into_iter().flatten() {
//             self.connections.remove(&address);
//         }
//         self.roundtrip_times.remove(peer_id);
//     }
// }
//
// impl NetworkBehaviour for SwarmApi {
//     type ProtocolsHandler = DummyProtocolsHandler;
//     type OutEvent = void::Void;
//
//     fn new_handler(&mut self) -> Self::ProtocolsHandler {
//         trace!("new_handler");
//         Default::default()
//     }
//
//     fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
//         trace!("addresses_of_peer {}", peer_id);
//         self.connected_peers
//             .get(peer_id)
//             .cloned()
//             .map(|addrs| addrs.into_iter().map(From::from).collect())
//             .unwrap_or_default()
//     }
//
//     fn inject_connection_established(
//         &mut self,
//         peer_id: &PeerId,
//         _id: &ConnectionId,
//         cp: &ConnectedPoint,
//     ) {
//         // TODO: could be that the connection is not yet fully established at this point
//         trace!("inject_connected {} {:?}", peer_id, cp);
//         let addr: MultiaddrWithoutPeerId = connection_point_addr(cp).to_owned().try_into().unwrap();
//
//         self.peers.insert(*peer_id);
//         let connections = self.connected_peers.entry(*peer_id).or_default();
//         connections.push(addr.clone());
//
//         self.connections.insert(addr.clone(), *peer_id);
//
//         if let ConnectedPoint::Dialer { .. } = cp {
//             let addr = MultiaddrWithPeerId {
//                 multiaddr: addr,
//                 peer_id: *peer_id,
//             };
//
//             self.connect_registry
//                 .finish_subscription(addr.into(), Ok(()));
//         }
//     }
//
//     fn inject_connected(&mut self, _peer_id: &PeerId) {
//         // we have at least one fully open connection and handler is running
//     }
//
//     fn inject_connection_closed(
//         &mut self,
//         peer_id: &PeerId,
//         _id: &ConnectionId,
//         cp: &ConnectedPoint,
//     ) {
//         trace!("inject_connection_closed {} {:?}", peer_id, cp);
//         let closed_addr = connection_point_addr(cp).to_owned().try_into().unwrap();
//
//         let became_empty = if let Some(connections) = self.connected_peers.get_mut(peer_id) {
//             if let Some(index) = connections.iter().position(|addr| *addr == closed_addr) {
//                 connections.swap_remove(index);
//             }
//             connections.is_empty()
//         } else {
//             false
//         };
//         if became_empty {
//             self.connected_peers.remove(peer_id);
//         }
//         self.connections.remove(&closed_addr);
//
//         if let ConnectedPoint::Dialer { .. } = cp {
//             let addr = MultiaddrWithPeerId::from((closed_addr, peer_id.to_owned()));
//
//             self.connect_registry
//                 .finish_subscription(addr.into(), Err("Connection reset by peer".to_owned()));
//         }
//     }
//
//     fn inject_disconnected(&mut self, peer_id: &PeerId) {
//         // in rust-libp2p 0.19 this at least will not be invoked for a peer we boot by banning it.
//         trace!("inject_disconnected: {}", peer_id);
//         self.mark_disconnected(peer_id);
//     }
//
//     fn inject_event(&mut self, _peer_id: PeerId, _connection: ConnectionId, _event: void::Void) {}
//
//     fn inject_addr_reach_failure(
//         &mut self,
//         peer_id: Option<&PeerId>,
//         addr: &Multiaddr,
//         error: &dyn std::error::Error,
//     ) {
//         trace!("inject_addr_reach_failure {} {}", addr, error);
//         if let Some(peer_id) = peer_id {
//             let addr: MultiaddrWithPeerId = if let Ok(addr) = addr.to_owned().try_into() {
//                 addr
//             } else {
//                 (
//                     MultiaddrWithoutPeerId::try_from(addr.to_owned()).unwrap(),
//                     peer_id.to_owned(),
//                 )
//                     .into()
//             };
//             self.connect_registry
//                 .finish_subscription(addr.into(), Err(error.to_string()));
//         }
//     }
//
//     fn poll(
//         &mut self,
//         _: &mut Context,
//         _: &mut impl PollParameters,
//     ) -> Poll<NetworkBehaviourAction> {
//         if let Some(event) = self.events.pop_front() {
//             Poll::Ready(event)
//         } else {
//             Poll::Pending
//         }
//     }
// }
//
// fn connection_point_addr(cp: &ConnectedPoint) -> &Multiaddr {
//     match cp {
//         ConnectedPoint::Dialer { address } => address,
//         ConnectedPoint::Listener { send_back_addr, .. } => send_back_addr,
//     }
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::p2p::transport::{build_transport, TTransport};
//     use libp2p::identity::Keypair;
//     use libp2p::{multiaddr::Protocol, multihash::Multihash, swarm::Swarm};
//     use std::convert::TryInto;
//
//     #[tokio::test]
//     async fn swarm_api() {
//         let (peer1_id, trans) = mk_transport();
//         let mut swarm1 = Swarm::new(trans, SwarmApi::default(), peer1_id);
//
//         let (peer2_id, trans) = mk_transport();
//         let mut swarm2 = Swarm::new(trans, SwarmApi::default(), peer2_id);
//
//         Swarm::listen_on(&mut swarm1, "/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
//
//         for l in Swarm::listeners(&swarm1) {
//             let mut addr = l.to_owned();
//             addr.push(Protocol::P2p(
//                 Multihash::from_bytes(&peer1_id.to_bytes()).unwrap(),
//             ));
//             if let Some(fut) = swarm2.connect(addr.try_into().unwrap()) {
//                 fut.await.unwrap();
//             }
//         }
//     }
//
//     fn mk_transport() -> (PeerId, TTransport) {
//         let key = Keypair::generate_ed25519();
//         let peer_id = key.public().into_peer_id();
//         let transport = build_transport(key).unwrap();
//         (peer_id, transport)
//     }
// }
