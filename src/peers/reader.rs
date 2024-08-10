use std::net::IpAddr;

use bitcoin::p2p::address::AddrV2;
use bitcoin::p2p::{message::NetworkMessage, message_blockdata::Inventory, ServiceFlags};
use bitcoin::Txid;
use tokio::sync::mpsc::Sender;

use crate::node::channel_messages::{CombinedAddr, PeerMessage};
use crate::node::messages::RejectPayload;

use super::error::PeerReadError;
use super::traits::MessageParser;

const ONE_MONTH: u64 = 2_500_000;
const ONE_MINUTE: u64 = 60;
const MAX_MESSAGE_BYTES: u32 = 1024 * 1024 * 32;
// From Bitcoin Core PR #29575
const MAX_ADDR: usize = 1_000;
const MAX_INV: usize = 50_000;
const MAX_HEADERS: usize = 2_000;

pub(crate) struct Reader {
    parser: Box<dyn MessageParser>,
    tx: Sender<PeerMessage>,
}

impl Reader {
    pub fn new(parser: impl MessageParser + 'static, tx: Sender<PeerMessage>) -> Self {
        Self {
            parser: Box::new(parser),
            tx,
        }
    }

    pub(crate) async fn read_from_remote(&mut self) -> Result<(), PeerReadError> {
        loop {
            if let Some(message) = self.parser.read_message().await? {
                let cleaned_message = self.parse_message(message);
                match cleaned_message {
                    Some(message) => self
                        .tx
                        .send(message)
                        .await
                        .map_err(|_| PeerReadError::MpscChannel)?,
                    None => continue,
                }
            }
        }
    }

    fn parse_message(&self, message: NetworkMessage) -> Option<PeerMessage> {
        match message {
            NetworkMessage::Version(version) => Some(PeerMessage::Version(version)),
            NetworkMessage::Verack => Some(PeerMessage::Verack),
            NetworkMessage::Addr(addresses) => {
                if addresses.len() > MAX_ADDR {
                    return Some(PeerMessage::Disconnect);
                }
                let addresses: Vec<CombinedAddr> = addresses
                    .iter()
                    .filter(|f| f.1.services.has(ServiceFlags::COMPACT_FILTERS))
                    .filter(|f| f.1.socket_addr().is_ok())
                    .map(|(_, addr)| {
                        let ip = match addr.socket_addr().unwrap().ip() {
                            IpAddr::V4(ip) => AddrV2::Ipv4(ip),
                            IpAddr::V6(ip) => AddrV2::Ipv6(ip),
                        };
                        let mut addr = CombinedAddr::new(ip, addr.port);
                        addr.services(addr.services);
                        addr
                    })
                    .collect();
                Some(PeerMessage::Addr(addresses))
            }
            NetworkMessage::Inv(inventory) => {
                if inventory.len() > MAX_INV {
                    return Some(PeerMessage::Disconnect);
                }
                let mut hashes = Vec::new();
                for i in inventory {
                    match i {
                        Inventory::Block(hash) => hashes.push(hash),
                        Inventory::CompactBlock(hash) => hashes.push(hash),
                        Inventory::WitnessBlock(hash) => hashes.push(hash),
                        _ => continue,
                    }
                }
                if !hashes.is_empty() {
                    Some(PeerMessage::NewBlocks(hashes))
                } else {
                    None
                }
            }
            NetworkMessage::GetData(_) => None,
            NetworkMessage::NotFound(_) => None,
            NetworkMessage::GetBlocks(_) => None,
            NetworkMessage::GetHeaders(_) => None,
            NetworkMessage::MemPool => None,
            NetworkMessage::Tx(_) => None,
            NetworkMessage::Block(block) => Some(PeerMessage::Block(block)),
            NetworkMessage::Headers(headers) => {
                if headers.len() > MAX_HEADERS {
                    return Some(PeerMessage::Disconnect);
                }
                Some(PeerMessage::Headers(headers))
            }
            NetworkMessage::SendHeaders => None,
            NetworkMessage::GetAddr => None,
            NetworkMessage::Ping(nonce) => Some(PeerMessage::Ping(nonce)),
            NetworkMessage::Pong(nonce) => Some(PeerMessage::Pong(nonce)),
            NetworkMessage::MerkleBlock(_) => None,
            NetworkMessage::FilterLoad(_) => None,
            NetworkMessage::FilterAdd(_) => None,
            NetworkMessage::FilterClear => None,
            NetworkMessage::GetCFilters(_) => None,
            NetworkMessage::CFilter(filter) => Some(PeerMessage::Filter(filter)),
            NetworkMessage::GetCFHeaders(_) => None,
            NetworkMessage::CFHeaders(cf_headers) => Some(PeerMessage::FilterHeaders(cf_headers)),
            NetworkMessage::GetCFCheckpt(_) => None,
            NetworkMessage::CFCheckpt(_) => None,
            NetworkMessage::SendCmpct(_) => None,
            NetworkMessage::CmpctBlock(_) => None,
            NetworkMessage::GetBlockTxn(_) => None,
            NetworkMessage::BlockTxn(_) => None,
            NetworkMessage::Alert(_) => None,
            NetworkMessage::Reject(rejection) => {
                let txid = Txid::from(rejection.hash);
                Some(PeerMessage::Reject(RejectPayload {
                    reason: rejection.ccode,
                    txid,
                }))
            }
            NetworkMessage::FeeFilter(_) => None,
            NetworkMessage::WtxidRelay => None,
            NetworkMessage::AddrV2(addresses) => {
                if addresses.len() > MAX_ADDR {
                    return Some(PeerMessage::Disconnect);
                }
                let addresses: Vec<CombinedAddr> = addresses
                    .iter()
                    .filter(|f| f.services.has(ServiceFlags::COMPACT_FILTERS))
                    .map(|addr| {
                        let mut ip = CombinedAddr::new(addr.addr.clone(), addr.port);
                        ip.services(addr.services);
                        ip
                    })
                    .collect();
                Some(PeerMessage::Addr(addresses))
            }
            NetworkMessage::SendAddrV2 => None,
            #[allow(unused)]
            NetworkMessage::Unknown { command, payload } => Some(PeerMessage::Disconnect),
        }
    }
}
