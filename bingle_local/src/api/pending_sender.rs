//! Background sender for pending outbound messages, shared by bingle_jsi and bingle_cli (issue #283).
//!
//! A client persists an outbound message as **pending** (`progress < 1.0`) in the local store and
//! calls [`PendingSender::wake`]; the sender does the rest. It follows the shared retry policy in
//! [`send_retry`](crate::api::send_retry): a transient failure keeps the message pending and it is
//! retried, with per-message backoff so one unreachable recipient cannot starve the others; a
//! permanent failure marks it terminal.
//!
//! Two threads do the work. The **scheduler** never blocks on the network: it picks the oldest
//! eligible pending message, hands it to the **worker**, and reaps the result, so it stays
//! responsive to [`wake`](PendingSender::wake) and [`stop`](PendingSender::stop) however long a send
//! takes. The worker sends one message at a time and records the outcome in the store. It works on
//! an [`OutboundStore`] handle rather than behind any client lock, so a slow send — or the
//! store-and-forward Mailbox post a failed send triggers — never blocks the client's other calls.
//!
//! After a send fails because the recipient is unreachable, the sender treats that recipient as
//! offline for [`PendingSenderOptions::offline_window`] (issue #278). With the store-and-forward send
//! gate on, sends to them inside the window skip the direct attempt — and its connect and relay
//! timeouts — and go straight to their Mailbox. A delivered send, or the client reporting that it
//! heard from the peer ([`PendingSender::peer_seen`]), ends the window early.
//!
//! On exit, a client calls [`PendingSender::shutdown`] rather than [`stop`](PendingSender::stop):
//! it lets an in-flight send finish, then hands every message still pending to its recipients'
//! Mailboxes, so a message queued just before exit is not left stranded (issue #282).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::{BingleApiBoth, BingleError, ProgressCallback, SendFailureKind};
use serde_json::{Value as JsonValue, json};

use crate::api::bingle_local_api::{BingleLocalApi, Message};
use crate::api::bingle_local_api_impl::BingleApiLocalImpl;
use crate::api::send_retry::{
    OFFLINE_WINDOW, OfflineWindow, RETRY_BACKOFF, SendFailure, classify_send_error,
    indicates_peer_offline, select_sendable_message,
};

/// The outbound side of the local store, as the [`PendingSender`] uses it. Every method takes
/// `&self`: implementations are shared, interior-mutable handles, so the sender never needs a
/// client's lock.
pub trait OutboundStore: Send + Sync {
    /// Outbound messages still awaiting delivery (`progress < 1.0`).
    fn pending_messages(&self) -> Result<Vec<Message>, BingleError>;

    /// Record a send attempt's progress or outcome. Recording a failure is what drives the
    /// store-and-forward post-on-give-up (#214) when the send gate is on, so this can make a
    /// network call.
    fn update_message_status(
        &self,
        id: &str,
        progress: f32,
        failure_reason: Option<String>,
        failure_kind: Option<SendFailureKind>,
    ) -> Result<(), BingleError>;

    /// Whether the store-and-forward SEND gate is on.
    fn store_and_forward_send(&self) -> bool;

    /// Whether the message with this id is complete with no failure — delivered, or handed off
    /// to the recipient's Mailbox.
    fn is_handed_off(&self, id: &str) -> bool;
}

/// An [`OutboundStore`] over a [`BingleApiLocalImpl`] handle that also saves the state file after
/// each status update, when one is configured.
#[derive(Clone)]
pub struct LocalOutboundStore {
    local: BingleApiLocalImpl,
    state_file: Option<PathBuf>,
}

impl LocalOutboundStore {
    /// Wrap a handle to the local store (see [`BingleApiLocalImpl`]'s `Clone`), saving to
    /// `state_file` after each update when it is `Some`.
    pub fn new(local: BingleApiLocalImpl, state_file: Option<PathBuf>) -> Self {
        Self { local, state_file }
    }
}

impl OutboundStore for LocalOutboundStore {
    fn pending_messages(&self) -> Result<Vec<Message>, BingleError> {
        self.local.get_pending_messages()
    }

    fn update_message_status(
        &self,
        id: &str,
        progress: f32,
        failure_reason: Option<String>,
        failure_kind: Option<SendFailureKind>,
    ) -> Result<(), BingleError> {
        self.local
            .update_message_status_shared(id, progress, failure_reason, failure_kind)?;
        if let Some(path) = &self.state_file
            && let Err(e) = self.local.save(path.to_string_lossy().as_ref())
        {
            tracing::warn!("[PendingSender] could not save state: {e}");
        }
        Ok(())
    }

    fn store_and_forward_send(&self) -> bool {
        self.local.store_and_forward_send()
    }

    fn is_handed_off(&self, id: &str) -> bool {
        self.local
            .get_messages()
            .ok()
            .into_iter()
            .flatten()
            .find(|m| m.id == id)
            .map(|m| m.progress == Some(1.0) && m.failure_reason.is_none())
            .unwrap_or(false)
    }
}

/// How the [`PendingSender`] delivers one message to one recipient. Abstracted so the sender is
/// testable without a live engine.
pub trait MessageDelivery: Send + Sync {
    /// Send `message` to `recipient` (the label stored on the message — normally a handle).
    /// `Ok(true)` is delivered; `Ok(false)` or an error is a failure, classified by
    /// [`classify_send_error`].
    fn deliver(
        &self,
        recipient: &str,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError>;
}

impl MessageDelivery for Arc<dyn BingleApiBoth> {
    fn deliver(
        &self,
        recipient: &str,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        self.send_message_to_handle(&recipient.to_string(), message, progress)
    }
}

/// What happened to a message on one attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// Delivered to every recipient; marked complete.
    Delivered,
    /// The direct send failed but, with the store-and-forward send gate on, the message was
    /// handed off to the recipients' Sidewinder Mailboxes and marked complete (#214, #272). Carries
    /// the human-readable reason the direct send failed.
    Forwarded(String),
    /// A transient failure: the message stays pending and will be retried. Carries the
    /// human-readable reason.
    Retrying(String),
    /// A permanent failure (or any failure with retries disabled): marked failed. Carries the
    /// human-readable reason.
    Failed(String),
}

/// One attempt's outcome, passed to the [`PendingSender`]'s outcome callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReport {
    /// The message's id (its key in the store, issue #209).
    pub id: String,
    /// The message's recipients.
    pub recipients: Vec<String>,
    /// What happened.
    pub outcome: SendOutcome,
    /// Whether an earlier attempt had already failed (the message carried a failure reason). Lets
    /// a client report a message's first failure, and a later recovery, without repeating itself
    /// on every retry.
    pub previously_failed: bool,
}

/// Tuning for a [`PendingSender`].
#[derive(Debug, Clone)]
pub struct PendingSenderOptions {
    /// How often the scheduler wakes to look for work when nothing else wakes it.
    pub tick: Duration,
    /// When `false` (`bingle_cli chat --no-retries`), every failure is permanent, so nothing is
    /// left pending to retry.
    pub retries_enabled: bool,
    /// A send taking longer than this is logged (once); the scheduler keeps running regardless.
    pub watchdog: Duration,
    /// How long a recipient is treated as offline after a send to them fails because they could not
    /// be reached (issue #278).
    pub offline_window: Duration,
}

impl PendingSenderOptions {
    /// Defaults matching the React Native client: a 200ms tick, retries on, a 45s watchdog, and
    /// the shared [`OFFLINE_WINDOW`].
    pub fn new() -> Self {
        Self {
            tick: Duration::from_millis(200),
            retries_enabled: true,
            watchdog: Duration::from_secs(45),
            offline_window: OFFLINE_WINDOW,
        }
    }
}

impl Default for PendingSenderOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Readiness check: whether the transport can deliver right now (for example, listening and not
/// in `NoConnection`). Nothing is handed to the worker while it returns `false`.
pub type ReadyCheck = dyn Fn() -> bool + Send + Sync;

/// Called with each attempt's outcome, on the scheduler thread.
pub type OutcomeCallback = dyn Fn(SendReport) + Send + Sync;

/// A message named in a [`ShutdownReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownEntry {
    /// The message's id (its key in the store, issue #209).
    pub id: String,
    /// The message's recipients.
    pub recipients: Vec<String>,
}

/// Why [`PendingSender::shutdown`] left a message pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotSentReason {
    /// The store-and-forward send gate is off, so there is no Mailbox to hand it to.
    StoreForwardOff,
    /// The post to the Mailbox did not complete (node unreachable, recipient not resolvable, …).
    PostFailed,
    /// Its direct send was still running at the deadline, so it was left alone rather than risk
    /// delivering it twice.
    InFlight,
    /// The deadline passed before it could be handed off.
    DeadlineReached,
}

/// What [`PendingSender::shutdown`] did with the messages still pending at exit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Messages handed off to their recipients' Mailboxes.
    pub forwarded: Vec<ShutdownEntry>,
    /// Messages left pending (they stay in the store and are retried by the next sender), with why.
    pub not_sent: Vec<(ShutdownEntry, NotSentReason)>,
}

/// The message the worker is sending, if any, with a condition variable signalled when it
/// finishes. Set by the scheduler before a hand-off (so a message can never be both in flight and
/// flushed), cleared by the worker once the outcome is recorded.
type InFlight = (Mutex<Option<String>>, Condvar);

/// Scheduler input.
enum Event {
    /// The worker finished a message.
    Done { id: String, report: SendReport },
    /// New work may be available; look now rather than at the next tick.
    Wake,
    /// Shut down.
    Stop,
}

/// The shared background sender for pending outbound messages. See the [module docs](self).
///
/// Started with [`start`](Self::start); stops on [`stop`](Self::stop) or when dropped.
pub struct PendingSender {
    events: Mutex<mpsc::Sender<Event>>,
    running: Arc<AtomicBool>,
    scheduler: Mutex<Option<JoinHandle<()>>>,
    /// Recipients recently found unreachable, shared with the worker (issue #278).
    offline: Arc<Mutex<OfflineWindow>>,
    /// The message being sent, shared with the scheduler and worker (issue #282).
    in_flight: Arc<InFlight>,
    /// The store, for the shutdown flush.
    store: Arc<dyn OutboundStore>,
}

impl PendingSender {
    /// Start the scheduler and worker threads.
    pub fn start(
        store: Arc<dyn OutboundStore>,
        delivery: Arc<dyn MessageDelivery>,
        options: PendingSenderOptions,
        ready: Arc<ReadyCheck>,
        on_outcome: Arc<OutcomeCallback>,
    ) -> PendingSender {
        let (event_tx, event_rx) = mpsc::channel::<Event>();
        let (work_tx, work_rx) = mpsc::channel::<Message>();
        let running = Arc::new(AtomicBool::new(true));

        let offline = Arc::new(Mutex::new(OfflineWindow::new(options.offline_window)));
        let worker_offline = Arc::clone(&offline);
        let in_flight: Arc<InFlight> = Arc::new((Mutex::new(None), Condvar::new()));
        let worker_in_flight = Arc::clone(&in_flight);
        let worker_store = Arc::clone(&store);
        let worker_events = event_tx.clone();
        let retries_enabled = options.retries_enabled;
        let spawned_worker = std::thread::Builder::new()
            .name("bingle-pending-sender".to_string())
            .spawn(move || {
                while let Ok(msg) = work_rx.recv() {
                    let id = msg.id.clone();
                    let report = send_one(
                        &worker_store,
                        &*delivery,
                        &worker_offline,
                        &msg,
                        retries_enabled,
                    );
                    // The outcome is recorded: the message is no longer in flight.
                    let (lock, finished) = &*worker_in_flight;
                    if let Ok(mut current) = lock.lock() {
                        *current = None;
                    }
                    finished.notify_all();
                    if worker_events.send(Event::Done { id, report }).is_err() {
                        break; // scheduler gone
                    }
                }
                tracing::debug!("[PendingSender] worker stopped");
            });
        if let Err(e) = spawned_worker {
            tracing::error!("[PendingSender] could not start the worker thread: {e}");
        }

        let scheduler_running = Arc::clone(&running);
        let scheduler_in_flight = Arc::clone(&in_flight);
        let scheduler_store = Arc::clone(&store);
        let scheduler = std::thread::Builder::new()
            .name("bingle-pending-scheduler".to_string())
            .spawn(move || {
                run_scheduler(
                    &*scheduler_store,
                    &scheduler_in_flight,
                    options,
                    &*ready,
                    &*on_outcome,
                    event_rx,
                    work_tx,
                    &scheduler_running,
                )
            });
        let scheduler = match scheduler {
            Ok(handle) => Some(handle),
            Err(e) => {
                tracing::error!("[PendingSender] could not start the scheduler thread: {e}");
                None
            }
        };

        PendingSender {
            events: Mutex::new(event_tx),
            running,
            scheduler: Mutex::new(scheduler),
            offline,
            in_flight,
            store,
        }
    }

    /// Stop, and flush what is still pending (issue #282). Call on exit instead of
    /// [`stop`](Self::stop).
    ///
    /// Stops the scheduler, then waits (up to `deadline`) for a send already in flight to finish, so
    /// no message is delivered twice. Then, with the store-and-forward send gate on, hands each
    /// message still pending to its recipients' Mailboxes, skipping the direct attempt. Messages it
    /// cannot hand off stay pending in the store, for the next sender to retry, and are reported
    /// with the reason. Posts already started are not interrupted, so the call can run over
    /// `deadline` by the time of one post.
    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport {
        let until = Instant::now() + deadline;
        self.stop();

        let (lock, finished) = &*self.in_flight;
        let still_in_flight = match lock.lock() {
            Ok(mut current) => {
                while current.is_some() {
                    let now = Instant::now();
                    if now >= until {
                        break;
                    }
                    match finished.wait_timeout(current, until - now) {
                        Ok((guard, _)) => current = guard,
                        Err(e) => {
                            current = e.into_inner().0;
                            break;
                        }
                    }
                }
                current.clone()
            }
            Err(_) => None,
        };

        let pending = match self.store.pending_messages() {
            Ok(pending) => pending,
            Err(e) => {
                tracing::error!("[PendingSender] shutdown could not read pending messages: {e}");
                Vec::new()
            }
        };
        let forwarding = self.store.store_and_forward_send();
        let mut report = ShutdownReport::default();
        for msg in pending {
            let entry = ShutdownEntry {
                id: msg.id.clone(),
                recipients: msg.recipient_handles.clone(),
            };
            let not_sent = if still_in_flight.as_deref() == Some(msg.id.as_str()) {
                Some(NotSentReason::InFlight)
            } else if !forwarding {
                Some(NotSentReason::StoreForwardOff)
            } else if Instant::now() >= until {
                Some(NotSentReason::DeadlineReached)
            } else {
                // Record it as unreachable: that drives the post to the recipients' Mailboxes. It
                // stays pending (retryable) if the post does not complete.
                let failure = classify_send_error(&Err(BingleError::Send {
                    kind: SendFailureKind::PeerUnreachable,
                    detail: "not delivered before exit".to_string(),
                }));
                if let Some(failure) = failure {
                    let _ = self.store.update_message_status(
                        &msg.id,
                        0.0,
                        Some(failure.reason),
                        Some(failure.kind),
                    );
                }
                (!self.store.is_handed_off(&msg.id)).then_some(NotSentReason::PostFailed)
            };
            match not_sent {
                None => report.forwarded.push(entry),
                Some(reason) => report.not_sent.push((entry, reason)),
            }
        }
        tracing::info!(
            "[PendingSender] shutdown: {} forwarded, {} left pending",
            report.forwarded.len(),
            report.not_sent.len()
        );
        report
    }

    /// The client heard from `recipient` (a handle or id) — a real-time message or one read from
    /// the Mailbox — so it is online: end its offline window, so the next send to it is attempted
    /// direct, and look for work now (issue #278).
    pub fn peer_seen(&self, recipient: &str) {
        if let Ok(mut offline) = self.offline.lock() {
            offline.clear(recipient);
        }
        self.wake();
    }

    /// Whether `recipient` is inside its offline window now.
    pub fn is_offline(&self, recipient: &str) -> bool {
        self.offline
            .lock()
            .map(|offline| offline.is_offline(recipient, Instant::now()))
            .unwrap_or(false)
    }

    /// Look for work now — call after queueing a message so its first attempt is immediate rather
    /// than at the next tick.
    pub fn wake(&self) {
        if let Ok(tx) = self.events.lock() {
            let _ = tx.send(Event::Wake);
        }
    }

    /// Stop the scheduler and wait for it to exit. The worker is not joined: a send in progress
    /// finishes (bounded by the send path's own timeouts) and the worker then exits, so stopping
    /// never blocks on a slow send. Idempotent.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Ok(tx) = self.events.lock() {
            let _ = tx.send(Event::Stop);
        }
        let handle = self.scheduler.lock().ok().and_then(|mut g| g.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

impl Drop for PendingSender {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_scheduler(
    store: &dyn OutboundStore,
    in_flight_shared: &InFlight,
    options: PendingSenderOptions,
    ready: &ReadyCheck,
    on_outcome: &OutcomeCallback,
    events: mpsc::Receiver<Event>,
    work: mpsc::Sender<Message>,
    running: &AtomicBool,
) {
    tracing::info!("[PendingSender] started");
    let mut in_flight: Option<(String, Instant)> = None;
    let mut warned_stuck = false;
    // Per-message backoff deadlines, so a repeatedly failing recipient yields the head of the queue.
    let mut retry_after: HashMap<String, Instant> = HashMap::new();

    while running.load(Ordering::SeqCst) {
        match events.recv_timeout(options.tick) {
            Ok(Event::Done { id, report }) => {
                match report.outcome {
                    SendOutcome::Retrying(_) => {
                        retry_after.insert(id.clone(), Instant::now() + RETRY_BACKOFF);
                    }
                    _ => {
                        retry_after.remove(&id);
                    }
                }
                if in_flight.as_ref().is_some_and(|(t, _)| *t == id) {
                    in_flight = None;
                    warned_stuck = false;
                }
                on_outcome(report);
            }
            Ok(Event::Wake) | Err(RecvTimeoutError::Timeout) => {}
            Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => break,
        }

        // Tolerate, but note, a send taking a long time. No second send is started meanwhile.
        if let Some((id, since)) = &in_flight
            && !warned_stuck
            && since.elapsed() > options.watchdog
        {
            tracing::warn!(
                "[PendingSender] message {} send exceeded {:?}; holding further sends until it completes",
                id,
                options.watchdog
            );
            warned_stuck = true;
        }

        if in_flight.is_none() && ready() {
            let pending = match store.pending_messages() {
                Ok(pending) => pending,
                Err(e) => {
                    tracing::error!("[PendingSender] could not read pending messages: {e}");
                    Vec::new()
                }
            };
            // Forget deadlines for messages no longer pending, so the map stays bounded.
            retry_after.retain(|id, _| pending.iter().any(|m| m.id == *id));
            if let Some(msg) = select_sendable_message(pending, &retry_after, Instant::now()) {
                tracing::debug!("[PendingSender] sending message {}", msg.id);
                in_flight = Some((msg.id.clone(), Instant::now()));
                // Mark it in flight before the hand-off, so a shutdown flush cannot also send it.
                if let Ok(mut current) = in_flight_shared.0.lock() {
                    *current = Some(msg.id.clone());
                }
                if work.send(msg).is_err() {
                    tracing::error!("[PendingSender] worker gone; stopping");
                    if let Ok(mut current) = in_flight_shared.0.lock() {
                        *current = None;
                    }
                    break;
                }
            }
        }
    }
    tracing::info!("[PendingSender] stopped");
}

/// Send one message to each of its recipients, record the outcome in the store, and report it.
///
/// Recipients are tried in order. A transient failure stops the attempt (the transport is likely
/// down, so the rest would fail too); a permanent one moves on to the next recipient. A panic in
/// the send path is contained and treated as a transient failure, so it cannot kill the worker.
///
/// With the store-and-forward send gate on, a recipient inside its offline window is not sent to
/// directly: the attempt counts as unreachable straight away, and recording that failure posts the
/// message to their Mailbox (issue #278).
fn send_one(
    store: &Arc<dyn OutboundStore>,
    delivery: &dyn MessageDelivery,
    offline: &Mutex<OfflineWindow>,
    msg: &Message,
    retries_enabled: bool,
) -> SendReport {
    let id = msg.id.as_str();
    let previously_failed = msg.failure_reason.is_some();
    let forwarding = store.store_and_forward_send();

    let mut last_failure: Option<SendFailure> = None;
    for recipient in &msg.recipient_handles {
        let recently_offline = forwarding
            && offline
                .lock()
                .map(|o| o.is_offline(recipient, Instant::now()))
                .unwrap_or(false);
        if recently_offline {
            tracing::debug!(
                "[PendingSender] {recipient} is offline; sending via store-and-forward"
            );
            last_failure = classify_send_error(&Err(BingleError::Send {
                kind: SendFailureKind::PeerUnreachable,
                detail: format!("{recipient} is offline (a recent send failed)"),
            }));
            break;
        }

        // Surface the send path's progress on the message, as the React Native client expects.
        let progress_store = Arc::clone(store);
        let progress_id = id.to_string();
        let progress: Arc<ProgressCallback> = Arc::new(move |percent: u8, _status: String| {
            let _ = progress_store.update_message_status(
                &progress_id,
                f32::from(percent) / 100.0,
                None,
                None,
            );
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // The queued timestamp is the signed send time (issue #94), so a Mailbox fallback seals
            // the same time.
            delivery.deliver(
                recipient,
                json!({ "text": msg.text, "sent_time": msg.timestamp }),
                Some(progress),
            )
        }))
        .unwrap_or_else(|_| {
            tracing::error!("[PendingSender] send to {recipient} panicked");
            Err(BingleError::Send {
                kind: SendFailureKind::NotReady,
                detail: "send panicked".to_string(),
            })
        });
        match classify_send_error(&result) {
            None => {
                if let Ok(mut o) = offline.lock() {
                    o.clear(recipient);
                }
            }
            Some(failure) => {
                if indicates_peer_offline(failure.kind) {
                    tracing::info!(
                        "[PendingSender] {recipient} is unreachable: {}",
                        failure.reason
                    );
                    if let Ok(mut o) = offline.lock() {
                        o.mark_offline(recipient, Instant::now());
                    }
                }
                let retryable = failure.kind.is_retryable();
                last_failure = Some(failure);
                if retryable {
                    break;
                }
            }
        }
    }

    let outcome = match last_failure {
        None => {
            let _ = store.update_message_status(id, 1.0, None, None);
            SendOutcome::Delivered
        }
        Some(failure) => {
            let transient = retries_enabled && failure.kind.is_retryable();
            let progress = if transient { 0.0 } else { 1.0 };
            tracing::debug!(
                "[PendingSender] message {} send failed ({}, {:?}): {}",
                id,
                if transient { "transient" } else { "permanent" },
                failure.kind,
                failure.reason
            );
            // Recording the failure drives the store-and-forward post when the send gate is on.
            let _ = store.update_message_status(
                id,
                progress,
                Some(failure.reason.clone()),
                Some(failure.kind),
            );
            if store.store_and_forward_send() && store.is_handed_off(id) {
                SendOutcome::Forwarded(failure.reason)
            } else if transient {
                SendOutcome::Retrying(failure.reason)
            } else {
                SendOutcome::Failed(failure.reason)
            }
        }
    };
    SendReport {
        id: id.to_string(),
        recipients: msg.recipient_handles.clone(),
        outcome,
        previously_failed,
    }
}
