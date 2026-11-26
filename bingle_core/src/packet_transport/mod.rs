use crate::api::bingle_api::{NetworkEndpoint, NetworkEndpointKey};
use crate::dtls::Dtls;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub mod frame;
pub mod reassembly;
pub mod send_window;

use frame::{
    DATA_FRAG_HEADER_LEN, FRPT_HEADER_LEN, FragHeader, Frame, PACKET_MAX, build_ack_complete,
    build_ack_window, build_data_frag, build_data_single,
};
use reassembly::{FragOutcome, Reassembler, ReassemblyLimits};
use send_window::SendWindow;

const DEFAULT_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
];

type AckWaiter = Arc<(Mutex<bool>, Condvar)>;
type FragWaiter = Arc<(Mutex<SendWindow>, Condvar)>;
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
    // Fragmented sends awaiting ACK_WINDOW / ACK_COMPLETE.
    pending_frag_sends: Arc<Mutex<HashMap<AckKey, FragWaiter>>>,
    // Blocks delivered upward, single or fragmented, for duplicate suppression.
    delivered_blocks: Arc<Mutex<HashSet<DeliveredKey>>>,
    // Partial DATA_FRAG transfers being reassembled.
    reassembly: Arc<Mutex<Reassembler>>,
    // Largest block passed to send so far, for the size diagnostics (issue #518).
    largest_block_sent: AtomicUsize,
    // Engine-level observer for fresh outbound DTLS sessions (issue #50). Set via
    // set_new_outbound_session_observer; invoked from the DTLS outbound-session hook.
    on_new_outbound_session: Arc<Mutex<Option<NewOutboundSessionObserver>>>,
}

/// The transport state an inbound packet is dispatched against, shared with the DTLS handler.
#[derive(Clone)]
struct InboundState {
    handle_message: Arc<Mutex<Option<PacketTransportHandleMessage>>>,
    endpoint_sessions: Arc<Mutex<HashMap<NetworkEndpointKey, SessionGeneration>>>,
    pending_acks: Arc<Mutex<HashMap<AckKey, AckWaiter>>>,
    pending_frag_sends: Arc<Mutex<HashMap<AckKey, FragWaiter>>>,
    delivered_blocks: Arc<Mutex<HashSet<DeliveredKey>>>,
    reassembly: Arc<Mutex<Reassembler>>,
}

/// The per-session state reset when a peer starts a new DTLS session.
#[derive(Clone)]
struct SessionState {
    endpoint_sessions: Arc<Mutex<HashMap<NetworkEndpointKey, SessionGeneration>>>,
    pending_acks: Arc<Mutex<HashMap<AckKey, AckWaiter>>>,
    pending_frag_sends: Arc<Mutex<HashMap<AckKey, FragWaiter>>>,
    delivered_blocks: Arc<Mutex<HashSet<DeliveredKey>>>,
    reassembly: Arc<Mutex<Reassembler>>,
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
            pending_frag_sends: Arc::new(Mutex::new(HashMap::new())),
            delivered_blocks: Arc::new(Mutex::new(HashSet::new())),
            reassembly: Arc::new(Mutex::new(Reassembler::new(ReassemblyLimits::standard()))),
            largest_block_sent: AtomicUsize::new(0),
            on_new_outbound_session: Arc::new(Mutex::new(None)),
        };

        let inbound_for_dtls = transport.inbound_state();
        transport
            .dtls
            .set_handle_message(Some(Arc::new(move |server, from, issuer, packet| {
                if let Err(e) =
                    Self::dispatch_inbound_packet(&inbound_for_dtls, server, from, issuer, packet)
                {
                    tracing::warn!(
                        "[DtlsReliablePacketTransport::new] packet transport handle_message failed: {}",
                        e
                    );
                }
            })));

        let session_for_new_session = transport.session_state();
        transport
            .dtls
            .set_handle_new_session(Some(Arc::new(move |endpoint| {
                if let Err(e) = Self::handle_new_dtls_session(&session_for_new_session, endpoint)
                {
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

    /// Replaces the reassembly resource limits (spec §6.4); applies to fragments received from now
    /// on. Partial transfers already held are not trimmed until their next fragment arrives.
    pub fn set_reassembly_limits(&self, limits: ReassemblyLimits) {
        match self.reassembly.lock() {
            Ok(mut r) => r.set_limits(limits),
            Err(e) => tracing::warn!(
                "[DtlsReliablePacketTransport::set_reassembly_limits] lock poisoned: {}",
                e
            ),
        }
    }

    /// `(partial transfers, bytes held)` in reassembly right now.
    pub fn reassembly_usage(&self) -> Result<(usize, usize), String> {
        let r = self.reassembly.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::reassembly_usage] lock poisoned: {}",
                e
            )
        })?;
        Ok((r.transfer_count(), r.bytes_in_use()))
    }

    /// Largest block passed to `send` so far (issue #518 diagnostics).
    pub fn largest_block_sent(&self) -> usize {
        self.largest_block_sent.load(Ordering::Relaxed)
    }

    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
    pub fn dispatch_handle_message(
        &self,
        from: &NetworkEndpoint,
        issuer: &str,
        packet: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        Self::dispatch_inbound_packet(&self.inbound_state(), self.dtls(), from, issuer, packet)
    }

    fn inbound_state(&self) -> InboundState {
        InboundState {
            handle_message: self.handle_message.clone(),
            endpoint_sessions: self.endpoint_sessions.clone(),
            pending_acks: self.pending_acks.clone(),
            pending_frag_sends: self.pending_frag_sends.clone(),
            delivered_blocks: self.delivered_blocks.clone(),
            reassembly: self.reassembly.clone(),
        }
    }

    fn session_state(&self) -> SessionState {
        SessionState {
            endpoint_sessions: self.endpoint_sessions.clone(),
            pending_acks: self.pending_acks.clone(),
            pending_frag_sends: self.pending_frag_sends.clone(),
            delivered_blocks: self.delivered_blocks.clone(),
            reassembly: self.reassembly.clone(),
        }
    }

    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
    pub fn on_new_session(&self, endpoint: &NetworkEndpoint) -> Result<(), String> {
        Self::handle_new_dtls_session(&self.session_state(), endpoint)
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
        session: &SessionState,
        endpoint: &NetworkEndpoint,
    ) -> Result<(), String> {
        let SessionState {
            endpoint_sessions,
            pending_acks,
            pending_frag_sends,
            delivered_blocks,
            reassembly,
        } = session;
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
            let mut pending = pending_frag_sends.lock().map_err(|e| {
                format!(
                    "[DtlsReliablePacketTransport::handle_new_dtls_session] failed to lock pending fragmented sends: {}",
                    e
                )
            })?;
            pending.retain(|(key, _, _), _| key != &endpoint_key);
        }

        {
            let mut r = reassembly.lock().map_err(|e| {
                format!(
                    "[DtlsReliablePacketTransport::handle_new_dtls_session] failed to lock reassembly state: {}",
                    e
                )
            })?;
            r.drop_peer(&endpoint_key);
        }

        {
            let mut delivered = delivered_blocks.lock().map_err(|e| {
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

    fn send_ack_complete(dtls: &dyn Dtls, to: &NetworkEndpoint, tx_id: u16) -> Result<(), String> {
        dtls.send(to, &build_ack_complete(tx_id))
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

    /// Drops the DTLS session to `to` and sends `packet` again, which starts a new handshake
    /// (issue #483).
    fn resend_on_fresh_session(&self, to: &NetworkEndpoint, packet: &[u8]) -> Result<(), String> {
        self.dtls.forget_peer(to);
        self.dtls.send(to, packet)
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

    fn deliver(
        handle_message: &Arc<Mutex<Option<PacketTransportHandleMessage>>>,
        dtls: &dyn Dtls,
        from: &NetworkEndpoint,
        issuer: &str,
        block: &[u8],
        kind: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        if let Some(handler) = Self::get_handler(handle_message, "dispatch_inbound_packet") {
            handler(dtls, from, issuer, block)
        } else {
            tracing::warn!(
                "[DtlsReliablePacketTransport::dispatch_inbound_packet] received {} but no packet transport handler configured",
                kind
            );
            Ok(None)
        }
    }

    /// Records `key` as delivered. Returns false if it already was (a duplicate).
    fn mark_delivered(
        delivered_blocks: &Arc<Mutex<HashSet<DeliveredKey>>>,
        key: DeliveredKey,
    ) -> Result<bool, String> {
        let mut delivered = delivered_blocks.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::dispatch_inbound_packet] failed to lock receive cache: {}",
                e
            )
        })?;
        Ok(delivered.insert(key))
    }

    fn is_delivered(
        delivered_blocks: &Arc<Mutex<HashSet<DeliveredKey>>>,
        key: &DeliveredKey,
    ) -> Result<bool, String> {
        let delivered = delivered_blocks.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::dispatch_inbound_packet] failed to lock receive cache: {}",
                e
            )
        })?;
        Ok(delivered.contains(key))
    }

    fn pending_frag_send(
        pending_frag_sends: &Arc<Mutex<HashMap<AckKey, FragWaiter>>>,
        key: &AckKey,
    ) -> Result<Option<FragWaiter>, String> {
        let pending = pending_frag_sends.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::dispatch_inbound_packet] failed to lock pending fragmented sends: {}",
                e
            )
        })?;
        Ok(pending.get(key).cloned())
    }

    /// Applies `update` to a fragmented send's window and wakes its sender.
    fn update_frag_send(
        waiter: &FragWaiter,
        update: impl FnOnce(&mut SendWindow),
    ) -> Result<(), String> {
        let (lock, condvar) = (&waiter.0, &waiter.1);
        let mut window = lock.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::dispatch_inbound_packet] failed to lock send window: {}",
                e
            )
        })?;
        update(&mut window);
        condvar.notify_all();
        Ok(())
    }

    fn handle_ack_complete(
        inbound: &InboundState,
        from: &NetworkEndpoint,
        ack_key: AckKey,
    ) -> Result<(), String> {
        let matched_single = Self::complete_pending_ack(&inbound.pending_acks, &ack_key)?;
        let matched_frag = match Self::pending_frag_send(&inbound.pending_frag_sends, &ack_key)? {
            Some(waiter) => {
                Self::update_frag_send(&waiter, |w| w.mark_complete())?;
                true
            }
            None => false,
        };
        let (from_key, from_generation, tx_id) = &ack_key;
        if matched_single || matched_frag {
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
        Ok(())
    }

    fn handle_data_frag(
        inbound: &InboundState,
        dtls: &dyn Dtls,
        from: &NetworkEndpoint,
        issuer: &str,
        key: DeliveredKey,
        header: &FragHeader,
        payload: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        // A fragment of a block already delivered: our ACK_COMPLETE was lost, so send it again.
        if Self::is_delivered(&inbound.delivered_blocks, &key)? {
            return Self::send_ack_complete(dtls, from, header.tx_id).map(|_| None);
        }

        let outcome = {
            let mut r = inbound.reassembly.lock().map_err(|e| {
                format!(
                    "[DtlsReliablePacketTransport::dispatch_inbound_packet] failed to lock reassembly state: {}",
                    e
                )
            })?;
            r.accept(key.clone(), header, payload, Instant::now())
        };

        match outcome {
            FragOutcome::Progress { acks } => {
                for (base_index, ack_bits) in acks {
                    dtls.send(from, &build_ack_window(header.tx_id, base_index, ack_bits))?;
                }
                Ok(None)
            }
            FragOutcome::Complete { block } => {
                tracing::debug!(
                    "[DtlsReliablePacketTransport::dispatch_inbound_packet] reassembled DATA_FRAG endpoint={} tx_id={} ({} bytes in {} fragments); sending ACK_COMPLETE",
                    from,
                    header.tx_id,
                    block.len(),
                    header.frag_count
                );
                Self::send_ack_complete(dtls, from, header.tx_id)?;
                if !Self::mark_delivered(&inbound.delivered_blocks, key)? {
                    return Ok(None);
                }
                Self::deliver(
                    &inbound.handle_message,
                    dtls,
                    from,
                    issuer,
                    &block,
                    "DATA_FRAG",
                )
            }
            FragOutcome::Rejected {
                reason,
                dropped_transfer,
            } => {
                if dropped_transfer {
                    tracing::warn!(
                        "[DtlsReliablePacketTransport::dispatch_inbound_packet] dropped DATA_FRAG transfer endpoint={} tx_id={}: {}",
                        from,
                        header.tx_id,
                        reason
                    );
                } else {
                    tracing::debug!(
                        "[DtlsReliablePacketTransport::dispatch_inbound_packet] ignored DATA_FRAG endpoint={} tx_id={} index={}: {}",
                        from,
                        header.tx_id,
                        header.frag_index,
                        reason
                    );
                }
                Ok(None)
            }
        }
    }

    fn dispatch_inbound_packet(
        inbound: &InboundState,
        dtls: &dyn Dtls,
        from: &NetworkEndpoint,
        issuer: &str,
        packet: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        let from_key = Self::endpoint_key(from, "dispatch_inbound_packet")?;
        let from_generation = Self::current_session_generation(
            &inbound.endpoint_sessions,
            &from_key,
            "dispatch_inbound_packet",
        )?;

        match frame::parse(packet) {
            None => Self::deliver(
                &inbound.handle_message,
                dtls,
                from,
                issuer,
                packet,
                "legacy packet",
            ),
            Some(Frame::Unsupported) => Ok(None),
            Some(Frame::AckComplete { tx_id }) => {
                Self::handle_ack_complete(inbound, from, (from_key, from_generation, tx_id))?;
                Ok(None)
            }
            Some(Frame::AckWindow {
                tx_id,
                base_index,
                ack_bits,
            }) => {
                let ack_key = (from_key, from_generation, tx_id);
                match Self::pending_frag_send(&inbound.pending_frag_sends, &ack_key)? {
                    Some(waiter) => Self::update_frag_send(&waiter, |w| {
                        w.apply_ack_window(base_index, ack_bits);
                    })?,
                    None => tracing::debug!(
                        "[DtlsReliablePacketTransport::dispatch_inbound_packet] ACK_WINDOW with no matching fragmented send endpoint={} tx_id={}",
                        from,
                        tx_id
                    ),
                }
                Ok(None)
            }
            Some(Frame::DataFrag { header, payload }) => Self::handle_data_frag(
                inbound,
                dtls,
                from,
                issuer,
                (from_key, from_generation, header.tx_id),
                &header,
                payload,
            ),
            Some(Frame::DataSingle { tx_id, payload }) => {
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

                if !Self::mark_delivered(
                    &inbound.delivered_blocks,
                    (from_key, from_generation, tx_id),
                )? {
                    return Ok(None);
                }
                Self::deliver(
                    &inbound.handle_message,
                    dtls,
                    from,
                    issuer,
                    payload,
                    "DATA_SINGLE",
                )
            }
        }
    }

    /// Sends a block that fits one packet as `DATA_SINGLE` and waits for `ACK_COMPLETE` (spec §5.2).
    fn send_single(&self, to: &NetworkEndpoint, block: &[u8]) -> Result<(), String> {
        let endpoint_key = Self::endpoint_key(to, "send")?;
        let generation =
            Self::current_session_generation(&self.endpoint_sessions, &endpoint_key, "send")?;

        let tx_id = self.next_tx_id()?;
        let packet = build_data_single(tx_id, block);
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
        //
        // Each retry starts a clean DTLS session (issue #483). A missing ACK_COMPLETE most often
        // means the session itself is dead, not that one datagram was lost: the peer's NAT moved
        // it to a new port, or the peer restarted, and the peer silently drops records for a
        // session it doesn't have. Re-sending on that session can't succeed, while a new
        // handshake reaches the peer wherever it now is, or fails fast if it is offline.
        let delays = self.retry_delays();
        for (attempt, delay) in delays.iter().enumerate() {
            match Self::wait_for_ack_complete_with_timeout(tx_id, &waiter, *delay) {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    // Per-attempt timeouts are expected while a peer is offline; only the final
                    // failure is reported, by the caller (issue #278).
                    tracing::debug!(
                        "[DtlsReliablePacketTransport::send] timed out waiting {:?} for ACK_COMPLETE key={} generation={} tx_id={} attempt={}",
                        delay,
                        ack_key.0,
                        ack_key.1,
                        tx_id,
                        attempt
                    );
                    if attempt + 1 < delays.len()
                        && let Err(e) = self.resend_on_fresh_session(to, &packet)
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

        // No ACK_COMPLETE on any session, the last one included. Forget the peer so the
        // caller's next send also starts with a fresh DTLS handshake rather than reusing a
        // session that is silently dead (issue #46).
        self.dtls.forget_peer(to);

        Err(format!(
            "[DtlsReliablePacketTransport::send] no ACK_COMPLETE received after {} retries for tx_id={}",
            delays.len(),
            tx_id
        ))
    }

    fn retry_delays(&self) -> Vec<Duration> {
        match self.retry_delays.lock() {
            Ok(d) => d.clone(),
            Err(e) => {
                tracing::warn!(
                    "[DtlsReliablePacketTransport::send] retry delay lock poisoned, using defaults: {}",
                    e
                );
                DEFAULT_RETRY_DELAYS.to_vec()
            }
        }
    }

    fn register_pending_frag_send(
        &self,
        ack_key: &AckKey,
        frag_count: u16,
    ) -> Result<FragWaiter, String> {
        let waiter = Arc::new((Mutex::new(SendWindow::new(frag_count)), Condvar::new()));
        let mut pending = self.pending_frag_sends.lock().map_err(|e| {
            format!(
                "[DtlsReliablePacketTransport::send] failed to lock pending fragmented sends: {}",
                e
            )
        })?;
        pending.insert(ack_key.clone(), waiter.clone());
        Ok(waiter)
    }

    fn clear_pending_frag_send(&self, ack_key: &AckKey) {
        match self.pending_frag_sends.lock() {
            Ok(mut pending) => {
                pending.remove(ack_key);
            }
            Err(e) => tracing::warn!(
                "[DtlsReliablePacketTransport::send] failed to clear pending fragmented send tx_id={}: {}",
                ack_key.2,
                e
            ),
        }
    }

    /// Sends a block too big for one packet as `DATA_FRAG`s and waits for `ACK_COMPLETE`
    /// (spec §5.3).
    fn send_fragmented(&self, to: &NetworkEndpoint, block: &[u8]) -> Result<(), String> {
        let frag_payload = self.mtu.saturating_sub(DATA_FRAG_HEADER_LEN);
        if frag_payload == 0 {
            return Err(format!(
                "[DtlsReliablePacketTransport::send] mtu {} is too small to fragment a {} byte block",
                self.mtu,
                block.len()
            ));
        }
        let frag_count = block.len().div_ceil(frag_payload);
        let frag_count = u16::try_from(frag_count).map_err(|_| {
            format!(
                "[DtlsReliablePacketTransport::send] block length {} needs {} fragments at mtu {}, more than {}",
                block.len(),
                frag_count,
                self.mtu,
                u16::MAX
            )
        })?;
        let block_len = u32::try_from(block.len()).map_err(|_| {
            format!(
                "[DtlsReliablePacketTransport::send] block length {} does not fit block_len",
                block.len()
            )
        })?;

        let endpoint_key = Self::endpoint_key(to, "send")?;
        let generation =
            Self::current_session_generation(&self.endpoint_sessions, &endpoint_key, "send")?;
        let tx_id = self.next_tx_id()?;
        let ack_key = (endpoint_key, generation, tx_id);
        tracing::info!(
            "[FRPT-ACK] sending DATA_FRAG to={:?} key={} generation={} tx_id={} ({} bytes in {} fragments); awaiting ACK_COMPLETE",
            to,
            ack_key.0,
            ack_key.1,
            tx_id,
            block.len(),
            frag_count,
        );

        let waiter = self.register_pending_frag_send(&ack_key, frag_count)?;
        let fragments = FragmentedBlock {
            block,
            tx_id,
            frag_count,
            block_len,
            frag_payload,
        };
        let result = self.run_fragmented_send(to, &fragments, &waiter);
        self.clear_pending_frag_send(&ack_key);

        match result {
            Ok(()) => Ok(()),
            Err(FragSendError::Timeout { timeouts }) => {
                // As for DATA_SINGLE (issue #46): the next send starts with a fresh handshake.
                self.dtls.forget_peer(to);
                Err(format!(
                    "[DtlsReliablePacketTransport::send] no ACK_COMPLETE received after {} retries for tx_id={} (DATA_FRAG)",
                    timeouts, tx_id
                ))
            }
            Err(FragSendError::Other(e)) => Err(e),
        }
    }

    /// The windowed send loop. Fragments go out within a 64-fragment window starting at the first
    /// unacknowledged fragment; each `ACK_WINDOW` slides the window. A timeout with no new
    /// acknowledgement retransmits the unacknowledged fragments in the window, on a fresh DTLS
    /// session if nothing at all has been acknowledged yet (issue #483). New acknowledgement
    /// restarts the retry schedule; the send fails after as many consecutive timeouts as there
    /// are retry delays.
    fn run_fragmented_send(
        &self,
        to: &NetworkEndpoint,
        fragments: &FragmentedBlock<'_>,
        waiter: &FragWaiter,
    ) -> Result<(), FragSendError> {
        let delays = self.retry_delays();
        let Some(first_delay) = delays.first().copied() else {
            return Err(FragSendError::Other(
                "[DtlsReliablePacketTransport::send] no retry delays configured".to_string(),
            ));
        };
        let (lock, condvar) = (&waiter.0, &waiter.1);
        let lock_err = |e: String| {
            FragSendError::Other(format!(
                "[DtlsReliablePacketTransport::send] failed to lock send window for tx_id={}: {}",
                fragments.tx_id, e
            ))
        };

        let mut next_unsent = 0usize;
        let mut timeouts = 0usize;
        let mut deadline = Instant::now() + first_delay;
        let mut seen_progress = 0u64;

        loop {
            let window_end = {
                let window = lock.lock().map_err(|e| lock_err(e.to_string()))?;
                if window.is_complete() {
                    return Ok(());
                }
                window.window_end()
            };
            while next_unsent < window_end {
                self.dtls
                    .send(to, &fragments.packet(next_unsent))
                    .map_err(FragSendError::Other)?;
                next_unsent += 1;
            }

            let window = lock.lock().map_err(|e| lock_err(e.to_string()))?;
            let timeout = deadline.saturating_duration_since(Instant::now());
            let (window, _) = condvar
                .wait_timeout_while(window, timeout, |w| {
                    !w.is_complete() && w.progress() == seen_progress
                })
                .map_err(|e| lock_err(e.to_string()))?;
            if window.is_complete() {
                return Ok(());
            }
            if window.progress() != seen_progress {
                seen_progress = window.progress();
                timeouts = 0;
                deadline = Instant::now() + first_delay;
                continue;
            }
            if Instant::now() < deadline {
                // Spurious wakeup.
                continue;
            }

            timeouts += 1;
            tracing::debug!(
                "[DtlsReliablePacketTransport::send] DATA_FRAG timeout tx_id={} acked={}/{} timeouts={}",
                fragments.tx_id,
                window.acked_count(),
                fragments.frag_count,
                timeouts
            );
            if timeouts >= delays.len() {
                return Err(FragSendError::Timeout { timeouts });
            }
            let fresh_session = !window.any_ack();
            let retransmit = window.unacked_in_window(next_unsent);
            drop(window);

            if fresh_session {
                self.dtls.forget_peer(to);
            }
            for index in retransmit {
                self.dtls
                    .send(to, &fragments.packet(index))
                    .map_err(FragSendError::Other)?;
            }
            deadline = Instant::now() + delays[timeouts];
        }
    }
}

/// A block being sent as fragments of `frag_payload` bytes.
struct FragmentedBlock<'a> {
    block: &'a [u8],
    tx_id: u16,
    frag_count: u16,
    block_len: u32,
    frag_payload: usize,
}

impl FragmentedBlock<'_> {
    fn packet(&self, index: usize) -> Vec<u8> {
        let start = index * self.frag_payload;
        let end = (start + self.frag_payload).min(self.block.len());
        build_data_frag(
            &FragHeader {
                tx_id: self.tx_id,
                frag_index: index as u16,
                frag_count: self.frag_count,
                block_len: self.block_len,
            },
            &self.block[start..end],
        )
    }
}

enum FragSendError {
    Timeout { timeouts: usize },
    Other(String),
}

impl PacketTransport for DtlsReliablePacketTransport {
    fn send(&self, to: &NetworkEndpoint, block: &[u8]) -> Result<(), String> {
        if block.len() > PACKET_MAX {
            return Err(format!(
                "[DtlsReliablePacketTransport::send] block length {} exceeds PACKET_MAX {}",
                block.len(),
                PACKET_MAX
            ));
        }

        let largest = self
            .largest_block_sent
            .fetch_max(block.len(), Ordering::Relaxed)
            .max(block.len());
        // Block size diagnostics (issue #518): what would not fit a smaller, IPv6-safe packet.
        tracing::debug!(
            "[DtlsReliablePacketTransport::send] block_len={} largest_so_far={} mtu={} to={}",
            block.len(),
            largest,
            self.mtu,
            to
        );

        // First release of multi-packet blocks (issue #518): anything that fits one packet still
        // goes as DATA_SINGLE, so receivers that predate DATA_FRAG see no change. Only blocks that
        // could not be sent at all before are fragmented.
        if block.len() + FRPT_HEADER_LEN <= self.mtu {
            self.send_single(to, block)
        } else {
            self.send_fragmented(to, block)
        }
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
