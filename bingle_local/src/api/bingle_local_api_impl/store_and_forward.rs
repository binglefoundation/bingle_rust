//! Store-and-forward posting on the [`BingleApiLocalImpl`]: the Sidewinder Mailbox client accessor,
//! the send/receive gate accessors, and the post-on-delivery-fail hook (epic #200, stories #212 /
//! #214). Split out of `bingle_local_api_impl` so the orchestration lives beside the sidechain seam
//! rather than in the core API file; as a child module it still reads the implementation's private
//! state (the keypair, config, and the persisted posted-set).

use super::BingleApiLocalImpl;
use crate::api::bingle_local_api::{BingleLocalApi, Message};
use crate::api::sidewinder;
use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::util::time::now_millis;
use std::collections::{BTreeMap, HashSet};

impl BingleApiLocalImpl {
    /// Build a Sidewinder [`Mailbox`](crate::api::sidewinder::Mailbox) client for the current
    /// keypair (store-and-forward, epic #200). The Mailbox signs its transactions with the same
    /// enrolled account as [`get_algo_ops`](BingleLocalApi::get_algo_ops).
    ///
    /// Returns `Err` when no Sidewinder node is configured (`config.sidewinder` is `None`), when no
    /// keypair is available, or when the endpoint/token is invalid — a surfaced error, never a
    /// panic. The post-on-fail (#214) and read-on-reconnect (#215) stories call this to reach the
    /// Mailbox.
    pub fn get_mailbox(&self) -> Result<sidewinder::Mailbox, BingleError> {
        let Some(config) = self.config.sidewinder.clone() else {
            return Err(BingleError::Other(
                "no sidewinder node configured for store-and-forward".to_string(),
            ));
        };
        let algo = self.get_algo_ops()?;
        sidewinder::Mailbox::new(algo, config)
    }

    /// Whether the send-side store-and-forward gate is on (epic #200, story #212): the post-on-fail
    /// path (#214) reads this and, when `false`, behaves exactly as today. Exposed so the gate is
    /// observable by that path and by tests.
    pub fn store_and_forward_send(&self) -> bool {
        self.config.store_and_forward_send
    }

    /// Whether the receive-side store-and-forward gate is on (epic #200, story #212): the
    /// read-on-reconnect path (#215) reads this and, when `false`, does no polling. Exposed so the
    /// gate is observable by that path and by tests.
    pub fn store_and_forward_receive(&self) -> bool {
        self.config.store_and_forward_receive
    }

    /// Test seam: record a `(timestamp, handle)` as already posted to a Mailbox, so the
    /// store-and-forward idempotency and persistence (#214) can be exercised without a live node.
    #[doc(hidden)]
    pub fn mark_forwarded_for_tests(&self, timestamp: i64, handle: &str) {
        if let Ok(mut g) = self.forwarded_messages.lock() {
            g.insert((timestamp, handle.to_string()));
        }
    }

    /// Test seam: the set of `(timestamp, handle)` pairs already posted to a Mailbox.
    #[doc(hidden)]
    pub fn forwarded_for_tests(&self) -> HashSet<(i64, String)> {
        self.forwarded_messages
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Post a message whose direct delivery failed to each recipient's Sidewinder Mailbox
    /// (store-and-forward post-on-delivery-fail, epic #200 story #214).
    ///
    /// Best-effort and never affects delivery: gated on the send toggle and a configured Sidewinder
    /// node, it seals the message to each recipient's identity key and `FIFO_APPEND`s it to their
    /// Mailbox. Idempotent per recipient — a `(timestamp, handle)` already recorded in
    /// `forwarded_messages` (persisted) is skipped, so a retry or restart does not double-post, and a
    /// recipient whose post failed is retried on the next call without re-posting the others. A
    /// missing keypair, an unresolvable handle, a seal failure, or a transport error is logged and
    /// skipped. `timestamp` is the message's send time, sealed in as the sender-stamped `sent_time`.
    ///
    /// Returns `true` when the message is now posted for **every** recipient (fully handed off to the
    /// sidechain), so the caller can stop retrying direct Bingle delivery; `false` while any recipient
    /// still needs a (retryable) post, or when the gate is off / the node is unconfigured.
    pub(crate) fn forward_message_to_mailbox(
        &self,
        timestamp: i64,
        recipient_handles: &[String],
        text: &str,
    ) -> bool {
        if !sidewinder::should_forward_send(
            self.config.store_and_forward_send,
            self.config.sidewinder.is_some(),
        ) {
            return false;
        }

        // Recipients of this message not yet posted to a Mailbox.
        let pending: Vec<String> = match self.forwarded_messages.lock() {
            Ok(guard) => {
                sidewinder::pending_forward_recipients(timestamp, recipient_handles, &guard)
            }
            Err(e) => {
                tracing::error!(
                    "[forward_to_mailbox] Failed to lock forwarded_messages: {}",
                    e
                );
                return false;
            }
        };
        // Every recipient already posted: the message is fully handed off to the sidechain.
        if pending.is_empty() {
            return true;
        }

        // The sender's Ed25519 private key signs the sealed envelope; without a keypair we cannot seal.
        let passphrase = match self.keypair.lock() {
            Ok(g) => g.as_ref().map(|k| k.passphrase.clone()),
            Err(e) => {
                tracing::error!("[forward_to_mailbox] Failed to lock keypair: {}", e);
                return false;
            }
        };
        let Some(passphrase) = passphrase else {
            tracing::warn!("[forward_to_mailbox] no keypair; skipping store-and-forward post");
            return false;
        };
        let private_key = match AlgoOps::seed_from_passphrase(&passphrase) {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!("[forward_to_mailbox] cannot derive signing key ({e}); skipping");
                return false;
            }
        };

        // Resolve handles on-chain and post through the sender's Mailbox client.
        let ops = match self.get_algo_ops() {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("[forward_to_mailbox] no keypair to post ({e}); skipping");
                return false;
            }
        };
        let bgl = AlgoBingle::new(ops, self.config.app_id, self.config.asset_id);
        let mailbox = match self.get_mailbox() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("[forward_to_mailbox] no mailbox configured ({e}); skipping");
                return false;
            }
        };

        for handle in pending {
            // Resolve the recipient handle to their Algorand address (their identity key).
            let address = match bgl.handle_lookup(&handle) {
                Ok(Some(a)) => a,
                Ok(None) => {
                    tracing::warn!(
                        "[forward_to_mailbox] no account for handle '{handle}'; skipping"
                    );
                    continue;
                }
                Err(e) => {
                    tracing::warn!(
                        "[forward_to_mailbox] handle lookup failed for '{handle}' ({e}); will retry"
                    );
                    continue;
                }
            };
            let recipient_pub = match algo_ops::address_to_byte_key(&address) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        "[forward_to_mailbox] bad recipient address '{address}' ({e}); skipping"
                    );
                    continue;
                }
            };
            let sealed = match bingle_core::crypto::sealed_envelope::seal_from_private_key(
                private_key,
                recipient_pub,
                timestamp,
                text,
            ) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!(
                        "[forward_to_mailbox] seal failed for '{handle}' ({e}); skipping"
                    );
                    continue;
                }
            };
            match mailbox.post(&address, &sealed) {
                Ok(()) => {
                    if let Ok(mut guard) = self.forwarded_messages.lock() {
                        guard.insert((timestamp, handle.clone()));
                    }
                    tracing::info!(
                        "[forward_to_mailbox] posted message {timestamp} to '{handle}' Mailbox"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "[forward_to_mailbox] Mailbox post to '{handle}' failed ({e}); will retry"
                    );
                }
            }
        }

        // Fully handed off only when every recipient is now posted.
        match self.forwarded_messages.lock() {
            Ok(guard) => recipient_handles
                .iter()
                .all(|h| guard.contains(&(timestamp, h.clone()))),
            Err(_) => false,
        }
    }

    /// Drain this user's Sidewinder Mailbox, decrypting and storing each held message
    /// (store-and-forward read-on-reconnect, epic #200 story #215). The
    /// [`poll_mailbox`](BingleLocalApi::poll_mailbox) trait method delegates here.
    ///
    /// Gated on the receive toggle and a configured Sidewinder node. `FIFO_REMOVE_HEAD` (`pop`)
    /// reads-and-drops the head in one operation, so a delivered message is not re-read on the next
    /// poll (at-most-once). Each message is opened with this account's private key
    /// (`sealed_envelope::unseal_with_private_key`), recorded on the local message store via
    /// [`Message::from_opened`] with `delivered_time` stamped now, and the returned batch is sorted by
    /// sender-stamped sent time (#69). Persist-before-drop is at-most-once: `pop` drops on the node
    /// first, so a crash before the next `save` loses that one message (acceptable for v0.0.3).
    ///
    /// Best-effort: the node being unreachable, a keypair being absent, or a message that fails to
    /// decrypt stops / skips without error. Returns the messages read this poll (possibly empty).
    pub(crate) fn poll_mailbox_inner(&self) -> Result<Vec<Message>, BingleError> {
        if !self.config.store_and_forward_receive || self.config.sidewinder.is_none() {
            return Ok(Vec::new());
        }

        // This account's Ed25519 private key opens envelopes sealed to it; without a keypair we
        // cannot read.
        let passphrase = match self.keypair.lock() {
            Ok(g) => g.as_ref().map(|k| k.passphrase.clone()),
            Err(e) => {
                tracing::error!("[poll_mailbox] Failed to lock keypair: {}", e);
                return Ok(Vec::new());
            }
        };
        let Some(passphrase) = passphrase else {
            tracing::warn!("[poll_mailbox] no keypair; skipping store-and-forward read");
            return Ok(Vec::new());
        };
        let private_key = match AlgoOps::seed_from_passphrase(&passphrase) {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!("[poll_mailbox] cannot derive opening key ({e}); skipping");
                return Ok(Vec::new());
            }
        };

        let ops = match self.get_algo_ops() {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("[poll_mailbox] no keypair to read ({e}); skipping");
                return Ok(Vec::new());
            }
        };
        let bgl = AlgoBingle::new(ops, self.config.app_id, self.config.asset_id);
        let mailbox = match self.get_mailbox() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("[poll_mailbox] no mailbox configured ({e}); skipping");
                return Ok(Vec::new());
            }
        };
        // Our own handle is the recipient of everything in our Mailbox.
        let own_handle = self.sender_handle().ok();

        // Accumulate into a structure kept ordered by sender-stamped sent time (#69). The key pairs
        // the sent time (falling back to the local delivered/arrival time) with a read-sequence
        // number, so messages with equal sent times keep their read order and none are dropped.
        let mut read: BTreeMap<(i64, u64), Message> = BTreeMap::new();
        let mut sequence: u64 = 0;
        loop {
            let bytes = match mailbox.pop() {
                Ok(Some(b)) => b,
                Ok(None) => break, // the Mailbox is drained
                Err(BingleError::Retryable(reason)) => {
                    tracing::warn!("[poll_mailbox] node not reachable/ready: {reason}; stopping");
                    break;
                }
                Err(e) => {
                    tracing::warn!("[poll_mailbox] pop failed ({e}); stopping");
                    break;
                }
            };
            let opened = match bingle_core::crypto::sealed_envelope::unseal_with_private_key(
                private_key,
                &bytes,
            ) {
                Ok(o) => o,
                Err(e) => {
                    // The message is already dropped from the node; we cannot recover it. Log and
                    // move on rather than aborting the drain.
                    tracing::warn!(
                        "[poll_mailbox] failed to open a mailbox message ({e}); dropping"
                    );
                    continue;
                }
            };

            // Attribute the message to the sender's handle, resolved from the authenticated sender
            // address; fall back to the address string when the handle cannot be resolved.
            let sender_address = algo_ops::byte_key_to_address(&opened.sender_id).ok();
            let sender_handle = sender_address
                .as_deref()
                .and_then(|addr| {
                    bgl.handle_for_address(self.config.app_id, addr)
                        .ok()
                        .flatten()
                })
                .or_else(|| sender_address.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let recipient_handles = own_handle.clone().map(|h| vec![h]).unwrap_or_default();
            let delivered_time = now_millis();
            let message =
                Message::from_opened(&opened, sender_handle, recipient_handles, delivered_time);

            // Persist-before-drop (at-most-once): record it locally now; the pop already removed it
            // from the node. The message store is written to disk on the caller's next `save`.
            if let Ok(mut guard) = self.messages.lock() {
                guard.push(message.clone());
            } else {
                tracing::error!(
                    "[poll_mailbox] Failed to lock messages; the message is not stored"
                );
            }
            let sort_time = message.sent_time.unwrap_or(message.timestamp);
            read.insert((sort_time, sequence), message);
            sequence += 1;
        }

        // The batch is already ordered by sent time by the map's key.
        Ok(read.into_values().collect())
    }
}
