use crate::bitswap_pb;
use crate::block::Block;
use crate::error::BitswapError;
use crate::prefix::Prefix;
use cid::Cid;
use prost::Message as ProstMessage;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::mem;

pub type Priority = i32;

/// The Ledger contains the history of transactions with a peer.
#[derive(Debug, Default)]
pub struct Ledger {
    /// The list of wanted blocks sent to the peer.
    sent_want_list: HashMap<Cid, Priority>,
    /// The list of wanted blocks received from the peer.
    pub(crate) received_want_list: HashMap<Cid, Priority>,
    /// Queued message.
    message: Message,
}

impl Ledger {
    /// Creates a new `PeerLedger`.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_block(&mut self, block: Block) {
        self.message.add_block(block);
    }

    pub fn want_block(&mut self, cid: &Cid, priority: Priority) {
        self.message.want_block(cid, priority);
    }

    pub fn cancel_block(&mut self, cid: &Cid) {
        self.message.cancel_block(cid);
    }

    /// Returns the blocks wanted by the peer in unspecified order
    pub fn wantlist(&self) -> Vec<(Cid, Priority)> {
        self.received_want_list
            .iter()
            .map(|(cid, prio)| (cid.clone(), *prio))
            .collect()
    }

    pub fn send(&mut self) -> Option<Message> {
        if self.message.is_empty() {
            return None;
        }
        // FIXME: this might produce too large message
        for cid in self.message.cancel() {
            self.sent_want_list.remove(cid);
        }
        for (cid, priority) in self.message.want() {
            self.sent_want_list.insert(cid.clone(), *priority);
        }

        Some(mem::take(&mut self.message))
    }
}

/// A bitswap message.
#[derive(Clone, PartialEq, Default)]
pub struct Message {
    /// List of wanted blocks.
    want: HashMap<Cid, Priority>,
    /// List of blocks to cancel.
    cancel: HashSet<Cid>,
    /// List of blocks which peer has
    haves: HashSet<Cid>,
    /// List of blocks which peer has not
    dont_haves: HashSet<Cid>,
    /// Whether it is the full list of wanted blocks.
    full: bool,
    /// List of blocks to send.
    pub(crate) blocks: Vec<Block>,
}

impl Message {
    /// Checks whether the queued message is empty.
    pub fn is_empty(&self) -> bool {
        self.want.is_empty() && self.cancel.is_empty() && self.blocks.is_empty()
    }

    /// Returns the list of blocks.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Returns the list of blocks.
    pub fn num_of_blocks(&self) -> usize {
        self.blocks.len()
    }

    /// Returns the list of blocks.
    pub fn bytes_of_blocks(&self) -> usize {
        self.blocks
            .iter()
            .fold(0, |acc, block| acc + block.data.len())
    }

    /// Returns the list of blocks, moves ownership.
    pub fn take_blocks(&mut self) -> Vec<Block> {
        mem::take(&mut self.blocks)
    }

    /// Returns the list of wanted blocks.
    pub fn want(&self) -> &HashMap<Cid, Priority> {
        &self.want
    }

    /// Returns the list of cancelled blocks.
    pub fn cancel(&self) -> &HashSet<Cid> {
        &self.cancel
    }

    /// Returns the list of blocks which have
    pub fn have(&self) -> &HashSet<Cid> {
        &self.haves
    }

    /// Returns the list of blocks which don't have
    pub fn dont_have(&self) -> &HashSet<Cid> {
        &self.dont_haves
    }

    /// Adds a block to the have list.
    pub fn have_block(&mut self, cid: &Cid) {
        self.haves.insert(cid.to_owned());
    }

    /// Adds a block to the don't have list.
    pub fn dont_have_block(&mut self, cid: &Cid) {
        self.dont_haves.insert(cid.to_owned());
    }

    /// Adds a `Block` to the message.
    pub fn add_block(&mut self, block: Block) {
        self.blocks.push(block);
    }

    /// Removes the block from the message.
    pub fn remove_block(&mut self, cid: &Cid) {
        self.blocks.retain(|block| block.cid() != cid);
    }

    /// Adds a block to the want list.
    pub fn want_block(&mut self, cid: &Cid, priority: Priority) {
        self.want.insert(cid.to_owned(), priority);
    }

    /// Adds a block to the cancel list.
    pub fn cancel_block(&mut self, cid: &Cid) {
        self.cancel.insert(cid.to_owned());
    }

    /// Removes the block from the want list.
    #[allow(unused)]
    pub fn remove_want_block(&mut self, cid: &Cid) {
        self.want.remove(cid);
    }
}

impl From<&Message> for Vec<u8> {
    fn from(msg: &Message) -> Vec<u8> {
        let mut proto = bitswap_pb::Message::default();
        let mut wantlist = bitswap_pb::message::Wantlist::default();
        for (cid, priority) in msg.want() {
            let entry = bitswap_pb::message::wantlist::Entry {
                block: cid.to_bytes(),
                priority: *priority,
                ..Default::default()
            };
            wantlist.entries.push(entry);
        }
        for cid in msg.cancel() {
            let entry = bitswap_pb::message::wantlist::Entry {
                block: cid.to_bytes(),
                cancel: true,
                ..Default::default()
            };
            wantlist.entries.push(entry);
        }
        for block in msg.blocks() {
            let payload = bitswap_pb::message::Block {
                prefix: Prefix::from(&block.cid).to_bytes(),
                data: block.data().to_vec(),
            };
            proto.payload.push(payload);
        }
        if !wantlist.entries.is_empty() {
            proto.wantlist = Some(wantlist);
        }
        let mut res = Vec::with_capacity(proto.encoded_len());
        proto
            .encode(&mut res)
            .expect("there is no situation in which the protobuf message can be invalid");
        res
    }
}

impl Message {
    /// Turns this `Message` into a message that can be sent to a substream.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.into()
    }

    /// Creates a `Message` from bytes that were received from a substream.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BitswapError> {
        Self::try_from(bytes)
    }
}

impl From<()> for Message {
    fn from(_: ()) -> Self {
        Default::default()
    }
}

impl TryFrom<&[u8]> for Message {
    type Error = BitswapError;
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let proto: bitswap_pb::Message = bitswap_pb::Message::decode(bytes)?;
        let mut message = Message::default();
        for entry in proto.wantlist.unwrap_or_default().entries {
            let cid = Cid::try_from(entry.block)?;
            if entry.cancel {
                message.cancel_block(&cid);
            } else {
                message.want_block(&cid, entry.priority);
            }
        }
        // block presences had added into bitswap proto when 2020.1
        // But it still doesn't work now in go-ipfs v0.7
        for bp in proto.block_presences {
            let cid = Cid::try_from(bp.cid)?;
            let msg_type = bitswap_pb::message::BlockPresenceType::from_i32(bp.r#type)
                .ok_or_else(|| BitswapError::InvalidData)?;

            match msg_type {
                bitswap_pb::message::BlockPresenceType::Have => {
                    message.have_block(&cid);
                }
                bitswap_pb::message::BlockPresenceType::DontHave => {
                    message.dont_have_block(&cid);
                }
            }
        }
        for payload in proto.payload {
            let prefix = Prefix::new(&payload.prefix)?;
            let cid = prefix.to_cid(&payload.data)?;
            let block = Block {
                cid,
                data: payload.data.into_boxed_slice(),
            };
            message.add_block(block);
        }
        Ok(message)
    }
}

impl std::fmt::Debug for Message {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let mut first = true;
        for (cid, priority) in self.want() {
            if first {
                first = false;
            } else {
                write!(fmt, ", ")?;
            }
            write!(fmt, "want: {} {}", cid, priority)?;
        }
        for cid in self.cancel() {
            if first {
                first = false;
            } else {
                write!(fmt, ", ")?;
            }
            write!(fmt, "cancel: {}", cid)?;
        }
        for block in self.blocks() {
            if first {
                first = false;
            } else {
                write!(fmt, ", ")?;
            }
            write!(fmt, "block: {}", block.cid())?;
        }

        if first {
            write!(fmt, "(empty message)")?;
        }

        Ok(())
    }
}
