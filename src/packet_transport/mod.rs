use crate::api::bingle_api::{NetworkEndpoint, NetworkEndpointKey};
use crate::dtls::Dtls;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

const FRPT_VERSION: u8 = 0x1;
const PACKET_TYPE_DATA_SINGLE: u8 = 0x1;
const PACKET_TYPE_ACK_COMPLETE: u8 = 0x4;
const FRPT_HEADER_LEN: usize = 4;

pub type PacketTransportHandleMessage = Arc<
    dyn Fn(&NetworkEndpoint, &str, &[u8]) -> Result<Option<Vec<u8>>, String> + Send + Sync,
>;

pub trait PacketTransport {
    fn send(&self, to: &NetworkEndpoint, block: &[u8]) -> Result<(), String>;

    fn get_handle_message(&self) -> Option<PacketTransportHandleMessage>;

    fn set_handle_message(&mut self, handler: Option<PacketTransportHandleMessage>);

    fn with_handle_message(self, handler: PacketTransportHandleMessage) -> Self
    where
        Self: Sized;
}

pub struct DtlsReliablePacketTransport {
    dtls: Box<dyn Dtls + Send + Sync>,
    mtu: usize,
    handle_message: Arc<Mutex<Option<PacketTransportHandleMessage>>>,
    next_tx_id: Arc<Mutex<u16>>,
    received_single_blocks: Arc<Mutex<HashSet<(NetworkEndpointKey, u16)>>>,
}

enum ParsedPacket<'a> {
    DataSingle { tx_id: u16, payload: &'a [u8] },
    AckComplete { tx_id: u16 },
    UnsupportedFrpt,
}

impl DtlsReliablePacketTransport {
    pub fn new(dtls: Box<dyn Dtls + Send + Sync>, mtu: usize) -> Self {
        let mut transport = Self {
            dtls,
            mtu,
            handle_message: Arc::new(Mutex::new(None)),
            next_tx_id: Arc::new(Mutex::new(0)),
            received_single_blocks: Arc::new(Mutex::new(HashSet::new())),
        };

        let handle_message_for_dtls = transport.handle_message.clone();
        let received_single_blocks_for_dtls = transport.received_single_blocks.clone();
        transport
            .dtls
            .set_handle_message(Some(Arc::new(move |server, from, issuer, packet| {
                if let Err(e) = Self::dispatch_inbound_packet(
                    &handle_message_for_dtls,
                    &received_single_blocks_for_dtls,
                    server,
                    from,
                    issuer,
                    packet,
                ) {
                    tracing::warn!(
                        "[DtlsReliablePacketTransport::new] packet transport handle_message failed: {}",
                        e
                    );
                }
            })));

        transport
    }

    pub fn dtls(&self) -> &(dyn Dtls + Send + Sync) {
        self.dtls.as_ref()
    }

    pub fn dtls_mut(&mut self) -> &mut (dyn Dtls + Send + Sync) {
        self.dtls.as_mut()
    }

    pub fn set_mtu(&mut self, mtu: usize) {
        self.mtu = mtu;
    }

    pub fn mtu(&self) -> usize {
        self.mtu
    }

    pub fn dispatch_handle_message(
        &self,
        from: &NetworkEndpoint,
        issuer: &str,
        packet: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        Self::dispatch_inbound_packet(
            &self.handle_message,
            &self.received_single_blocks,
            self.dtls(),
            from,
            issuer,
            packet,
        )
    }

    fn get_handler(
        handle_message: &Arc<Mutex<Option<PacketTransportHandleMessage>>>,
        context: &str,
    ) -> Option<PacketTransportHandleMessage> {
        match handle_message.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                tracing::warn!(
                    "[DtlsReliablePacketTransport::{}] failed to lock handler: {}",
                    context,
                    e
                );
                None
            }
        }
    }

    fn build_header(packet_type: u8, tx_id: u16) -> [u8; FRPT_HEADER_LEN] {
        [
            ((FRPT_VERSION & 0x0F) << 4) | (packet_type & 0x0F),
            0,
            (tx_id >> 8) as u8,
            (tx_id & 0xFF) as u8,
        ]
    }

    fn build_data_single_packet(tx_id: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(FRPT_HEADER_LEN + payload.len());
        out.extend_from_slice(&Self::build_header(PACKET_TYPE_DATA_SINGLE, tx_id));
        out.extend_from_slice(payload);
        out
    }

    fn parse_packet(packet: &[u8]) -> Option<ParsedPacket<'_>> {
        if packet.len() < FRPT_HEADER_LEN {
            return None;
        }

        let ver_type = packet[0];
        let version = ver_type >> 4;
        if version != FRPT_VERSION {
            return None;
        }

        let packet_type = ver_type & 0x0F;
        let tx_id = u16::from_be_bytes([packet[2], packet[3]]);

        match packet_type {
            PACKET_TYPE_DATA_SINGLE => Some(ParsedPacket::DataSingle {
                tx_id,
                payload: &packet[FRPT_HEADER_LEN..],
            }),
            PACKET_TYPE_ACK_COMPLETE if packet.len() == FRPT_HEADER_LEN => {
                Some(ParsedPacket::AckComplete { tx_id })
            }
            _ => Some(ParsedPacket::UnsupportedFrpt),
        }
    }

    fn endpoint_key(from: &NetworkEndpoint) -> Option<NetworkEndpointKey> {
        if let Some(inet_socket_address) = from.inet_socket_address() {
            return Some(NetworkEndpointKey {
                inet_socket_address: Some(inet_socket_address),
                relay_id: None,
                relay_channel: None,
            });
        }

        if let (Some(relay_id), Some(relay_channel)) = (from.relay_id(), from.relay_channel()) {
            return Some(NetworkEndpointKey {
                inet_socket_address: None,
                relay_id: Some(relay_id.to_string()),
                relay_channel: Some(relay_channel),
            });
        }

        None
    }

    fn send_ack_complete(
        dtls: &dyn Dtls,
        to: &NetworkEndpoint,
        tx_id: u16,
    ) -> Result<(), String> {
        let ack = Self::build_header(PACKET_TYPE_ACK_COMPLETE, tx_id);
        dtls.send(to, &ack)
    }

    fn next_tx_id(&self) -> Result<u16, String> {
        let mut next_tx_id = self.next_tx_id.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::send] failed to lock tx id state: {}",
                e
            )
        })?;
        let tx_id = *next_tx_id;
        *next_tx_id = next_tx_id.wrapping_add(1);
        Ok(tx_id)
    }

    fn dispatch_inbound_packet(
        handle_message: &Arc<Mutex<Option<PacketTransportHandleMessage>>>,
        received_single_blocks: &Arc<Mutex<HashSet<(NetworkEndpointKey, u16)>>>,
        dtls: &dyn Dtls,
        from: &NetworkEndpoint,
        issuer: &str,
        packet: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        match Self::parse_packet(packet) {
            None => {
                if let Some(handler) = Self::get_handler(handle_message, "dispatch_inbound_packet") {
                    handler(from, issuer, packet)
                } else {
                    tracing::warn!(
                        "[DtlsReliablePacketTransport::dispatch_inbound_packet] received legacy packet but no packet transport handler configured"
                    );
                    Ok(None)
                }
            }
            Some(ParsedPacket::UnsupportedFrpt) => Ok(None),
            Some(ParsedPacket::AckComplete { tx_id }) => {
                tracing::debug!(
                    "[DtlsReliablePacketTransport::dispatch_inbound_packet] received ACK_COMPLETE for tx_id={}",
                    tx_id
                );
                Ok(None)
            }
            Some(ParsedPacket::DataSingle { tx_id, payload }) => {
                Self::send_ack_complete(dtls, from, tx_id)?;

                let should_deliver = if let Some(from_key) = Self::endpoint_key(from) {
                    let mut delivered = received_single_blocks.lock().map_err(|e| {
                        format!(
                            "[DtlsReliablePacketTransport::dispatch_inbound_packet] failed to lock receive cache: {}",
                            e
                        )
                    })?;
                    delivered.insert((from_key, tx_id))
                } else {
                    tracing::warn!(
                        "[DtlsReliablePacketTransport::dispatch_inbound_packet] endpoint key unavailable; duplicate suppression disabled for this DATA_SINGLE"
                    );
                    true
                };

                if !should_deliver {
                    return Ok(None);
                }

                if let Some(handler) = Self::get_handler(handle_message, "dispatch_inbound_packet") {
                    handler(from, issuer, payload)
                } else {
                    tracing::warn!(
                        "[DtlsReliablePacketTransport::dispatch_inbound_packet] received DATA_SINGLE but no packet transport handler configured"
                    );
                    Ok(None)
                }
            }
        }
    }
}

impl PacketTransport for DtlsReliablePacketTransport {
    fn send(&self, to: &NetworkEndpoint, block: &[u8]) -> Result<(), String> {
        if block.len() + FRPT_HEADER_LEN > self.mtu {
            return Err(format!(
                "[DtlsReliablePacketTransport::send] block length {} exceeds DATA_SINGLE capacity {} for mtu {}",
                block.len(),
                self.mtu.saturating_sub(FRPT_HEADER_LEN),
                self.mtu
            ));
        }

        let tx_id = self.next_tx_id()?;
        let packet = Self::build_data_single_packet(tx_id, block);
        self.dtls.send(to, &packet)
    }

    fn get_handle_message(&self) -> Option<PacketTransportHandleMessage> {
        match self.handle_message.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                tracing::warn!(
                    "[DtlsReliablePacketTransport::get_handle_message] failed to lock handler: {}",
                    e
                );
                None
            }
        }
    }

    fn set_handle_message(&mut self, handler: Option<PacketTransportHandleMessage>) {
        match self.handle_message.lock() {
            Ok(mut g) => {
                *g = handler;
            }
            Err(e) => {
                tracing::warn!(
                    "[DtlsReliablePacketTransport::set_handle_message] failed to lock handler: {}",
                    e
                );
            }
        }
    }

    fn with_handle_message(mut self, handler: PacketTransportHandleMessage) -> Self
    where
        Self: Sized,
    {
        self.set_handle_message(Some(handler));
        self
    }
}