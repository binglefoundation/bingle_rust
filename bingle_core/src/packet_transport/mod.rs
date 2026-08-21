use crate::api::bingle_api::{NetworkEndpoint, NetworkEndpointKey};
use crate::dtls::Dtls;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const FRPT_VERSION: u8 = 0x1;
const PACKET_TYPE_DATA_SINGLE: u8 = 0x1;
const PACKET_TYPE_ACK_COMPLETE: u8 = 0x4;
const FRPT_HEADER_LEN: usize = 4;
const DEFAULT_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
];

type AckWaiter = Arc<(Mutex<bool>, Condvar)>;
type SessionGeneration = u64;
type AckKey = (NetworkEndpointKey, SessionGeneration, u16);
type DeliveredKey = (NetworkEndpointKey, SessionGeneration, u16);

pub type PacketTransportHandleMessage = Arc<
    dyn Fn(&dyn Dtls, &NetworkEndpoint, &str, &[u8]) -> Result<Option<Vec<u8>>, String>
        + Send
        + Sync,
>;

/// Observer invoked (with the peer endpoint) when a fresh outbound DTLS session
/// is established for that peer — a reconnect signal the engine uses to refresh
/// relay registration (issue #50).
pub type NewOutboundSessionObserver = Arc<dyn Fn(&NetworkEndpoint) + Send + Sync>;

pub trait PacketTransport {
    fn send(&self, to: &NetworkEndpoint, block: &[u8]) -> Result<(), String>;

    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
    fn get_handle_message(&self) -> Option<PacketTransportHandleMessage>;

    fn set_handle_message(&self, handler: Option<PacketTransportHandleMessage>);

    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
    fn with_handle_message(self, handler: PacketTransportHandleMessage) -> Self
    where
        Self: Sized;
}

pub struct DtlsReliablePacketTransport {
    dtls: Box<dyn Dtls + Send + Sync>,
    mtu: usize,
    // Interior-mutable so set_retry_delays can be &self (the owning API shares the transport's
    // Engine via Arc). Only read briefly to clone the schedule at the start of send().
    retry_delays: Mutex<Vec<Duration>>,
    handle_message: Arc<Mutex<Option<PacketTransportHandleMessage>>>,
    next_tx_id: Arc<Mutex<u16>>,
    endpoint_sessions: Arc<Mutex<HashMap<NetworkEndpointKey, SessionGeneration>>>,
    pending_acks: Arc<Mutex<HashMap<AckKey, AckWaiter>>>,
    received_single_blocks: Arc<Mutex<HashSet<DeliveredKey>>>,
    // Engine-level observer for fresh outbound DTLS sessions (issue #50). Set via
    // set_new_outbound_session_observer; invoked from the DTLS outbound-session hook.
    on_new_outbound_session: Arc<Mutex<Option<NewOutboundSessionObserver>>>,
}

enum ParsedPacket<'a> {
    DataSingle { tx_id: u16, payload: &'a [u8] },
    AckComplete { tx_id: u16 },
    UnsupportedFrpt,
}

impl DtlsReliablePacketTransport {
    pub fn new(dtls: Box<dyn Dtls + Send + Sync>, mtu: usize) -> Self {
        let transport = Self {
            dtls,
            mtu,
            retry_delays: Mutex::new(DEFAULT_RETRY_DELAYS.to_vec()),
            handle_message: Arc::new(Mutex::new(None)),
            next_tx_id: Arc::new(Mutex::new(0)),
            endpoint_sessions: Arc::new(Mutex::new(HashMap::new())),
            pending_acks: Arc::new(Mutex::new(HashMap::new())),
            received_single_blocks: Arc::new(Mutex::new(HashSet::new())),
            on_new_outbound_session: Arc::new(Mutex::new(None)),
        };

        let handle_message_for_dtls = transport.handle_message.clone();
        let endpoint_sessions_for_dtls = transport.endpoint_sessions.clone();
        let pending_acks_for_dtls = transport.pending_acks.clone();
        let received_single_blocks_for_dtls = transport.received_single_blocks.clone();
        transport
            .dtls
            .set_handle_message(Some(Arc::new(move |server, from, issuer, packet| {
                if let Err(e) = Self::dispatch_inbound_packet(
                    &handle_message_for_dtls,
                    &endpoint_sessions_for_dtls,
                    &pending_acks_for_dtls,
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

        let endpoint_sessions_for_new_session = transport.endpoint_sessions.clone();
        let pending_acks_for_new_session = transport.pending_acks.clone();
        let received_single_blocks_for_new_session = transport.received_single_blocks.clone();
        transport
            .dtls
            .set_handle_new_session(Some(Arc::new(move |endpoint| {
                if let Err(e) = Self::handle_new_dtls_session(
                    &endpoint_sessions_for_new_session,
                    &pending_acks_for_new_session,
                    &received_single_blocks_for_new_session,
                    endpoint,
                ) {
                    tracing::warn!(
                        "[DtlsReliablePacketTransport::new] failed to reset state for new DTLS session: {}",
                        e
                    );
                }
            })));

        // Forward fresh-outbound-session events to the engine observer, if one is
        // installed. This carries no reliable-transport side effects (issue #50).
        let on_new_outbound_for_dtls = transport.on_new_outbound_session.clone();
        transport
            .dtls
            .set_handle_new_outbound_session(Some(Arc::new(move |endpoint| {
                let observer = on_new_outbound_for_dtls.lock().ok().and_then(|g| g.clone());
                if let Some(observer) = observer {
                    observer(endpoint);
                }
            })));

        transport
    }

    /// Install the observer invoked when a fresh outbound DTLS session is
    /// established to a peer. The engine uses this to refresh relay registration
    /// on reconnect (issue #50). Replaces any previously installed observer.
    pub fn set_new_outbound_session_observer(&self, observer: Option<NewOutboundSessionObserver>) {
        match self.on_new_outbound_session.lock() {
            Ok(mut g) => *g = observer,
            Err(e) => tracing::warn!(
                "[DtlsReliablePacketTransport::set_new_outbound_session_observer] lock poisoned: {}",
                e
            ),
        }
    }

    pub fn dtls(&self) -> &(dyn Dtls + Send + Sync) {
        self.dtls.as_ref()
    }

    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
    pub fn set_mtu(&mut self, mtu: usize) {
        self.mtu = mtu;
    }

    pub fn set_retry_delays(&self, delays: Vec<Duration>) {
        *self.retry_delays.lock().unwrap() = delays;
    }

    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
    pub fn mtu(&self) -> usize {
        self.mtu
    }

    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
    pub fn dispatch_handle_message(
        &self,
        from: &NetworkEndpoint,
        issuer: &str,
        packet: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        Self::dispatch_inbound_packet(
            &self.handle_message,
            &self.endpoint_sessions,
            &self.pending_acks,
            &self.received_single_blocks,
            self.dtls(),
            from,
            issuer,
            packet,
        )
    }

    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
    pub fn on_new_session(&self, endpoint: &NetworkEndpoint) -> Result<(), String> {
        Self::handle_new_dtls_session(
            &self.endpoint_sessions,
            &self.pending_acks,
            &self.received_single_blocks,
            endpoint,
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

    fn next_session_generation(current: SessionGeneration) -> SessionGeneration {
        let mut next = current.wrapping_add(1);
        if next == 0 {
            next = 1;
        }
        next
    }

    fn endpoint_key(
        endpoint: &NetworkEndpoint,
        context: &str,
    ) -> Result<NetworkEndpointKey, String> {
        endpoint.get_key().ok_or_else(|| {
            format!(
                "[DtlsReliablePacketTransport::{}] endpoint key unavailable",
                context
            )
        })
    }

    fn current_session_generation(
        endpoint_sessions: &Arc<Mutex<HashMap<NetworkEndpointKey, SessionGeneration>>>,
        endpoint_key: &NetworkEndpointKey,
        context: &str,
    ) -> Result<SessionGeneration, String> {
        let mut sessions = endpoint_sessions.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::{}] failed to lock session state: {}",
                context, e
            )
        })?;
        let generation = sessions.entry(endpoint_key.clone()).or_insert(0);
        Ok(*generation)
    }

    fn handle_new_dtls_session(
        endpoint_sessions: &Arc<Mutex<HashMap<NetworkEndpointKey, SessionGeneration>>>,
        pending_acks: &Arc<Mutex<HashMap<AckKey, AckWaiter>>>,
        received_single_blocks: &Arc<Mutex<HashSet<DeliveredKey>>>,
        endpoint: &NetworkEndpoint,
    ) -> Result<(), String> {
        let endpoint_key = Self::endpoint_key(endpoint, "handle_new_dtls_session")?;

        let new_generation = {
            let mut sessions = endpoint_sessions.lock().map_err(|e| {
                format!(
                    "[DtlsReliablePacketTransport::handle_new_dtls_session] failed to lock session state: {}",
                    e
                )
            })?;
            let generation = sessions.entry(endpoint_key.clone()).or_insert(0);
            *generation = Self::next_session_generation(*generation);
            *generation
        };

        {
            let mut pending = pending_acks.lock().map_err(|e| {
                format!(
                    "[DtlsReliablePacketTransport::handle_new_dtls_session] failed to lock pending ACK state: {}",
                    e
                )
            })?;
            pending.retain(|(key, _, _), _| key != &endpoint_key);
        }

        {
            let mut delivered = received_single_blocks.lock().map_err(|e| {
                format!(
                    "[DtlsReliablePacketTransport::handle_new_dtls_session] failed to lock receive cache: {}",
                    e
                )
            })?;
            delivered.retain(|(key, _, _)| key != &endpoint_key);
        }

        tracing::debug!(
            "[DtlsReliablePacketTransport::handle_new_dtls_session] endpoint={} generation={}",
            endpoint,
            new_generation
        );

        Ok(())
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

    fn send_ack_complete(dtls: &dyn Dtls, to: &NetworkEndpoint, tx_id: u16) -> Result<(), String> {
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

    fn register_pending_ack(&self, ack_key: &AckKey) -> Result<AckWaiter, String> {
        let waiter = Arc::new((Mutex::new(false), Condvar::new()));
        let mut pending_acks = self.pending_acks.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::send] failed to lock pending ACK state: {}",
                e
            )
        })?;
        pending_acks.insert(ack_key.clone(), waiter.clone());
        Ok(waiter)
    }

    fn clear_pending_ack(&self, ack_key: &AckKey) -> Result<(), String> {
        let mut pending_acks = self.pending_acks.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::send] failed to lock pending ACK state: {}",
                e
            )
        })?;
        pending_acks.remove(ack_key);
        Ok(())
    }

    fn wait_for_ack_complete_with_timeout(
        tx_id: u16,
        waiter: &AckWaiter,
        timeout: Duration,
    ) -> Result<bool, String> {
        let (ack_lock, ack_condvar) = (&waiter.0, &waiter.1);
        let ack_received = ack_lock.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::send] failed to lock ACK waiter state for tx_id={}: {}",
                tx_id, e
            )
        })?;

        let (ack_received_after_wait, wait_result) = ack_condvar
            .wait_timeout_while(ack_received, timeout, |ack_complete| !*ack_complete)
            .map_err(|e| {
                format!(
                    "[DtlsReliablePacketTransport::send] failed while waiting for ACK_COMPLETE for tx_id={}: {}",
                    tx_id, e
                )
            })?;

        if *ack_received_after_wait {
            return Ok(true);
        }

        if wait_result.timed_out() {
            return Ok(false);
        }

        Err(format!(
            "[DtlsReliablePacketTransport::send] ACK_COMPLETE wait ended unexpectedly for tx_id={}",
            tx_id
        ))
    }

    fn complete_pending_ack(
        pending_acks: &Arc<Mutex<HashMap<AckKey, AckWaiter>>>,
        ack_key: &AckKey,
    ) -> Result<bool, String> {
        let waiter = {
            let mut pending_ack_guard = pending_acks.lock().map_err(|e| {
                format!(
                    "[DtlsReliablePacketTransport::dispatch_inbound_packet] failed to lock pending ACK state: {}",
                    e
                )
            })?;
            pending_ack_guard.remove(ack_key)
        };

        if let Some(waiter) = waiter {
            let (ack_lock, ack_condvar) = (&waiter.0, &waiter.1);
            let mut ack_received = ack_lock.lock().map_err(|e| {
                let (_, _, tx_id) = ack_key;
                format!(
                    "[DtlsReliablePacketTransport::dispatch_inbound_packet] failed to lock ACK waiter state for tx_id={}: {}",
                    tx_id, e
                )
            })?;
            *ack_received = true;
            ack_condvar.notify_one();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn dispatch_inbound_packet(
        handle_message: &Arc<Mutex<Option<PacketTransportHandleMessage>>>,
        endpoint_sessions: &Arc<Mutex<HashMap<NetworkEndpointKey, SessionGeneration>>>,
        pending_acks: &Arc<Mutex<HashMap<AckKey, AckWaiter>>>,
        received_single_blocks: &Arc<Mutex<HashSet<DeliveredKey>>>,
        dtls: &dyn Dtls,
        from: &NetworkEndpoint,
        issuer: &str,
        packet: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        let from_key = Self::endpoint_key(from, "dispatch_inbound_packet")?;
        let from_generation = Self::current_session_generation(
            endpoint_sessions,
            &from_key,
            "dispatch_inbound_packet",
        )?;

        match Self::parse_packet(packet) {
            None => {
                if let Some(handler) = Self::get_handler(handle_message, "dispatch_inbound_packet")
                {
                    handler(dtls, from, issuer, packet)
                } else {
                    tracing::warn!(
                        "[DtlsReliablePacketTransport::dispatch_inbound_packet] received legacy packet but no packet transport handler configured"
                    );
                    Ok(None)
                }
            }
            Some(ParsedPacket::UnsupportedFrpt) => Ok(None),
            Some(ParsedPacket::AckComplete { tx_id }) => {
                let ack_key = (from_key.clone(), from_generation, tx_id);
                let matched_pending_ack = Self::complete_pending_ack(pending_acks, &ack_key)?;
                if matched_pending_ack {
                    // Diagnostics (issue #82): logged at info so it shows in the same INFO capture as
                    // the matching send, to confirm the round-trip completed on the sender.
                    tracing::info!(
                        "[FRPT-ACK] received ACK_COMPLETE (matched pending send) endpoint={} key={} generation={} tx_id={}",
                        from,
                        from_key,
                        from_generation,
                        tx_id,
                    );
                } else {
                    // An ACK arrived but matches no pending send — e.g. it came back keyed by a
                    // different endpoint (direct vs relay channel) than the send registered, or the
                    // send already gave up. Warn (issue #82) so it is visible without --debug: this
                    // is the signature of an ACK key mismatch versus genuine non-delivery.
                    tracing::warn!(
                        "[FRPT-ACK] received ACK_COMPLETE with NO matching pending send endpoint={} key={} generation={} tx_id={}",
                        from,
                        from_key,
                        from_generation,
                        tx_id,
                    );
                }
                Ok(None)
            }
            Some(ParsedPacket::DataSingle { tx_id, payload }) => {
                // Diagnostics (issue #82): on the receiver (e.g. the mobile peer) this fires when a
                // reliable data packet arrives, just before we ACK it — confirming the relay actually
                // delivered the send and that we are about to reply.
                tracing::info!(
                    "[FRPT-ACK] received DATA_SINGLE endpoint={} key={} generation={} tx_id={}; sending ACK_COMPLETE",
                    from,
                    from_key,
                    from_generation,
                    tx_id,
                );
                Self::send_ack_complete(dtls, from, tx_id)?;

                let should_deliver = {
                    let mut delivered = received_single_blocks.lock().map_err(|e| {
                        format!(
                            "[DtlsReliablePacketTransport::dispatch_inbound_packet] failed to lock receive cache: {}",
                            e
                        )
                    })?;
                    delivered.insert((from_key, from_generation, tx_id))
                };

                if !should_deliver {
                    return Ok(None);
                }

                if let Some(handler) = Self::get_handler(handle_message, "dispatch_inbound_packet")
                {
                    handler(dtls, from, issuer, payload)
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

        let endpoint_key = Self::endpoint_key(to, "send")?;
        let generation =
            Self::current_session_generation(&self.endpoint_sessions, &endpoint_key, "send")?;

        let tx_id = self.next_tx_id()?;
        let packet = Self::build_data_single_packet(tx_id, block);
        let ack_key = (endpoint_key, generation, tx_id);
        // Diagnostics (issue #82): record the exact (endpoint key, generation, tx_id) we will wait
        // on, so an ACK that comes back keyed differently (e.g. relay channel vs direct address) is
        // easy to spot against the "[FRPT-ACK] received ACK_COMPLETE ..." lines.
        tracing::info!(
            "[FRPT-ACK] sending DATA_SINGLE to={:?} key={} generation={} tx_id={} ({} bytes); awaiting ACK_COMPLETE",
            to,
            ack_key.0,
            ack_key.1,
            tx_id,
            block.len(),
        );
        let waiter = self.register_pending_ack(&ack_key)?;

        if let Err(e) = self.dtls.send(to, &packet) {
            if let Err(cleanup_err) = self.clear_pending_ack(&ack_key) {
                tracing::warn!(
                    "[DtlsReliablePacketTransport::send] failed to clear pending ACK after send error for tx_id={}: {}",
                    tx_id,
                    cleanup_err
                );
            }
            return Err(e);
        }

        // Wait for ACK with retry backoff: try each delay in sequence, re-sending on timeout.
        // After all delays are exhausted without an ACK, fail with an error.
        let delays = self.retry_delays.lock().unwrap().clone();
        for (attempt, delay) in delays.iter().enumerate() {
            match Self::wait_for_ack_complete_with_timeout(tx_id, &waiter, *delay) {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    tracing::warn!(
                        "[DtlsReliablePacketTransport::send] timed out waiting {:?} for ACK_COMPLETE key={} generation={} tx_id={} attempt={}",
                        delay,
                        ack_key.0,
                        ack_key.1,
                        tx_id,
                        attempt
                    );
                    if attempt + 1 < delays.len()
                        && let Err(e) = self.dtls.send(to, &packet)
                    {
                        if let Err(cleanup_err) = self.clear_pending_ack(&ack_key) {
                            tracing::warn!(
                                "[DtlsReliablePacketTransport::send] failed to clear pending ACK after retry send error for tx_id={}: {}",
                                tx_id,
                                cleanup_err
                            );
                        }
                        return Err(e);
                    }
                }
                Err(e) => {
                    if let Err(cleanup_err) = self.clear_pending_ack(&ack_key) {
                        tracing::warn!(
                            "[DtlsReliablePacketTransport::send] failed to clear pending ACK after wait failure for tx_id={}: {}",
                            tx_id,
                            cleanup_err
                        );
                    }
                    return Err(e);
                }
            }
        }

        if let Err(cleanup_err) = self.clear_pending_ack(&ack_key) {
            tracing::warn!(
                "[DtlsReliablePacketTransport::send] failed to clear pending ACK after all retries exhausted for tx_id={}: {}",
                tx_id,
                cleanup_err
            );
        }

        // Every retry re-sent the same block on the same DTLS session and still saw no
        // ACK_COMPLETE. The local write kept "succeeding" so the session looks healthy, but
        // nothing is coming back: it is silently dead — e.g. after a network change to a
        // non-home root relay whose session is never re-established. Forget the peer so the
        // caller's next retry performs a fresh DTLS handshake and can recover, instead of
        // reusing the dead session forever. See issue #46.
        self.dtls.forget_peer(to);

        Err(format!(
            "[DtlsReliablePacketTransport::send] no ACK_COMPLETE received after {} retries for tx_id={}",
            delays.len(),
            tx_id
        ))
    }

    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
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

    fn set_handle_message(&self, handler: Option<PacketTransportHandleMessage>) {
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

    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
    fn with_handle_message(self, handler: PacketTransportHandleMessage) -> Self
    where
        Self: Sized,
    {
        self.set_handle_message(Some(handler));
        self
    }
}
