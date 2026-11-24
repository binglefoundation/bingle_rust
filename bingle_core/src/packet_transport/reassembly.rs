//! Receiver-side reassembly of `DATA_FRAG` transfers (spec §6.3–6.4).
//!
//! The reassembler is pure state: the transport passes each fragment in with the current time and
//! performs the returned actions (send `ACK_WINDOW`s, send `ACK_COMPLETE` and deliver the block).
//! Memory is allocated per fragment as fragments arrive, never up front from the announced
//! `block_len`, and is bounded by per-peer and global budgets.

use super::frame::{ACK_WINDOW_SPAN, FragHeader, PACKET_MAX};
use crate::api::bingle_api::NetworkEndpointKey;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

/// Identifies one transfer: the sending peer, its DTLS session generation and the `tx_id`.
pub type TransferKey = (NetworkEndpointKey, u64, u16);

/// Default bytes one peer may hold in partial reassemblies (16 MiB). This is also the largest
/// fragmented block a receiver accepts until the limit is raised.
pub const DEFAULT_PER_PEER_REASSEMBLY_BYTES: usize = 16 * 1024 * 1024;
/// Default bytes all peers together may hold in partial reassemblies (64 MiB).
pub const DEFAULT_GLOBAL_REASSEMBLY_BYTES: usize = 64 * 1024 * 1024;
/// Default number of transfers one peer may have in progress at once.
pub const DEFAULT_MAX_TRANSFERS_PER_PEER: usize = 8;
/// Default lifetime of a partial transfer, measured from its first fragment (spec §6.4).
pub const DEFAULT_REASSEMBLY_TIMEOUT: Duration = Duration::from_secs(30);
/// An `ACK_WINDOW` is held back at most this long after the previous one for the transfer
/// (spec §7.2), measured when the next fragment arrives.
pub const ACK_COALESCE: Duration = Duration::from_millis(25);
/// An `ACK_WINDOW` goes out after this many new fragments even inside the coalescing interval, so
/// the sender's 64-fragment window keeps moving.
pub const ACK_EVERY_FRAGMENTS: u16 = 16;

/// Resource limits for reassembly (spec §6.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReassemblyLimits {
    pub per_peer_bytes: usize,
    pub global_bytes: usize,
    pub max_transfers_per_peer: usize,
    pub timeout: Duration,
}

impl ReassemblyLimits {
    pub fn standard() -> Self {
        Self {
            per_peer_bytes: DEFAULT_PER_PEER_REASSEMBLY_BYTES,
            global_bytes: DEFAULT_GLOBAL_REASSEMBLY_BYTES,
            max_transfers_per_peer: DEFAULT_MAX_TRANSFERS_PER_PEER,
            timeout: DEFAULT_REASSEMBLY_TIMEOUT,
        }
    }
}

/// What the transport should do after a fragment is accepted.
#[derive(Debug, PartialEq, Eq)]
pub enum FragOutcome {
    /// The fragment is held (or was a duplicate). Send each `(base_index, ack_bits)` as an
    /// `ACK_WINDOW`; the list is empty while acknowledgements are being coalesced.
    Progress { acks: Vec<(u16, u64)> },
    /// The block is complete: send `ACK_COMPLETE`, then deliver it.
    Complete { block: Vec<u8> },
    /// The fragment was refused. `dropped_transfer` is true when a partial transfer was discarded
    /// as a result (worth a warning); otherwise only this packet was ignored.
    Rejected {
        reason: String,
        dropped_transfer: bool,
    },
}

struct Transfer {
    frag_count: u16,
    block_len: u32,
    /// Payload length of every fragment but the last, learnt from the first such fragment.
    frag_len: Option<usize>,
    fragments: BTreeMap<u16, Vec<u8>>,
    bytes: usize,
    started: Instant,
    last_ack: Option<Instant>,
    new_since_ack: u16,
    dirty_regions: BTreeSet<u16>,
}

impl Transfer {
    fn new(header: &FragHeader, now: Instant) -> Self {
        Self {
            frag_count: header.frag_count,
            block_len: header.block_len,
            frag_len: None,
            fragments: BTreeMap::new(),
            bytes: 0,
            started: now,
            last_ack: None,
            new_since_ack: 0,
            dirty_regions: BTreeSet::new(),
        }
    }

    fn last_index(&self) -> u16 {
        self.frag_count - 1
    }

    /// Length the last fragment must have once the regular fragment length is known.
    fn expected_last_len(&self, frag_len: usize) -> usize {
        self.block_len as usize - (self.frag_count as usize - 1) * frag_len
    }

    /// Checks `payload_len` for fragment `index` against the lengths seen so far, learning the
    /// regular fragment length from the first non-last fragment.
    fn check_length(&mut self, index: u16, payload_len: usize) -> Result<(), String> {
        let count = self.frag_count as usize;
        let block_len = self.block_len as usize;
        if index == self.last_index() {
            match self.frag_len {
                Some(frag_len) if payload_len != self.expected_last_len(frag_len) => Err(format!(
                    "last fragment is {} bytes, expected {}",
                    payload_len,
                    self.expected_last_len(frag_len)
                )),
                // Every other fragment carries at least one byte.
                None if payload_len > block_len - (count - 1) => Err(format!(
                    "last fragment of {} bytes leaves too little of block_len {} for {} fragments",
                    payload_len, block_len, count
                )),
                _ => Ok(()),
            }
        } else {
            match self.frag_len {
                Some(frag_len) if payload_len != frag_len => Err(format!(
                    "fragment {} is {} bytes, expected {}",
                    index, payload_len, frag_len
                )),
                Some(_) => Ok(()),
                None => {
                    // block_len must fall in ((count - 1) * len, count * len].
                    if (count - 1) * payload_len >= block_len || count * payload_len < block_len {
                        return Err(format!(
                            "fragment length {} does not fit block_len {} in {} fragments",
                            payload_len, block_len, count
                        ));
                    }
                    if let Some(last) = self.fragments.get(&self.last_index()) {
                        let expected = block_len - (count - 1) * payload_len;
                        if last.len() != expected {
                            return Err(format!(
                                "stored last fragment is {} bytes, expected {}",
                                last.len(),
                                expected
                            ));
                        }
                    }
                    self.frag_len = Some(payload_len);
                    Ok(())
                }
            }
        }
    }

    fn ack_bits(&self, region: u16) -> (u16, u64) {
        let base = region as usize * ACK_WINDOW_SPAN;
        let end = (base + ACK_WINDOW_SPAN).min(self.frag_count as usize);
        let mut bits = 0u64;
        for index in self
            .fragments
            .range(base as u16..=(end - 1) as u16)
            .map(|(i, _)| *i)
        {
            bits |= 1u64 << (index as usize - base);
        }
        (base as u16, bits)
    }

    fn take_acks(&mut self, now: Instant) -> Vec<(u16, u64)> {
        let regions = std::mem::take(&mut self.dirty_regions);
        let acks = regions.iter().map(|r| self.ack_bits(*r)).collect();
        self.last_ack = Some(now);
        self.new_since_ack = 0;
        acks
    }

    fn ack_due(&self, now: Instant) -> bool {
        match self.last_ack {
            None => true,
            Some(last) => {
                now.saturating_duration_since(last) >= ACK_COALESCE
                    || self.new_since_ack >= ACK_EVERY_FRAGMENTS
            }
        }
    }

    fn assemble(self) -> Vec<u8> {
        let mut block = Vec::with_capacity(self.block_len as usize);
        for (_, fragment) in self.fragments {
            block.extend_from_slice(&fragment);
        }
        block
    }
}

/// Partial `DATA_FRAG` transfers for every peer, with their resource limits.
pub struct Reassembler {
    limits: ReassemblyLimits,
    transfers: HashMap<TransferKey, Transfer>,
}

impl Reassembler {
    pub fn new(limits: ReassemblyLimits) -> Self {
        Self {
            limits,
            transfers: HashMap::new(),
        }
    }

    pub fn limits(&self) -> &ReassemblyLimits {
        &self.limits
    }

    pub fn set_limits(&mut self, limits: ReassemblyLimits) {
        self.limits = limits;
    }

    /// Partial transfers currently held.
    pub fn transfer_count(&self) -> usize {
        self.transfers.len()
    }

    /// Fragment payload bytes currently held across all partial transfers.
    pub fn bytes_in_use(&self) -> usize {
        self.transfers.values().map(|t| t.bytes).sum()
    }

    /// Fragment payload bytes currently held for one peer.
    pub fn peer_bytes(&self, peer: &NetworkEndpointKey) -> usize {
        self.transfers
            .iter()
            .filter(|((p, _, _), _)| p == peer)
            .map(|(_, t)| t.bytes)
            .sum()
    }

    /// Drops every partial transfer from `peer`, e.g. when it starts a new DTLS session.
    pub fn drop_peer(&mut self, peer: &NetworkEndpointKey) {
        self.transfers.retain(|(p, _, _), _| p != peer);
    }

    /// Drops partial transfers older than the timeout. Returns how many were dropped.
    pub fn expire(&mut self, now: Instant) -> usize {
        let timeout = self.limits.timeout;
        let before = self.transfers.len();
        self.transfers
            .retain(|_, t| now.saturating_duration_since(t.started) < timeout);
        before - self.transfers.len()
    }

    /// Validates a fragment's metadata on its own, before any state is touched.
    fn validate_header(&self, header: &FragHeader, payload_len: usize) -> Result<(), String> {
        let block_len = header.block_len as usize;
        if header.frag_count < 2 {
            return Err(format!("frag_count {} is below 2", header.frag_count));
        }
        if header.frag_index >= header.frag_count {
            return Err(format!(
                "frag_index {} is not below frag_count {}",
                header.frag_index, header.frag_count
            ));
        }
        if block_len > PACKET_MAX {
            return Err(format!(
                "block_len {} exceeds PACKET_MAX {}",
                block_len, PACKET_MAX
            ));
        }
        if block_len < header.frag_count as usize {
            return Err(format!(
                "block_len {} is smaller than frag_count {}",
                block_len, header.frag_count
            ));
        }
        if payload_len == 0 || payload_len > block_len {
            return Err(format!(
                "fragment payload of {} bytes is invalid for block_len {}",
                payload_len, block_len
            ));
        }
        // A block bigger than a budget could never complete, so refuse it before holding any of it.
        if block_len > self.limits.per_peer_bytes || block_len > self.limits.global_bytes {
            return Err(format!(
                "block_len {} exceeds the reassembly budget (per peer {}, global {})",
                block_len, self.limits.per_peer_bytes, self.limits.global_bytes
            ));
        }
        Ok(())
    }

    /// Among transfers other than `keep` matching `filter`, the key of the oldest.
    fn oldest_other(
        &self,
        keep: &TransferKey,
        filter: impl Fn(&TransferKey) -> bool,
    ) -> Option<TransferKey> {
        self.transfers
            .iter()
            .filter(|(k, _)| *k != keep && filter(k))
            .min_by_key(|(_, t)| t.started)
            .map(|(k, _)| k.clone())
    }

    /// Among transfers other than `keep`, the key of the one holding the most bytes.
    fn largest_other(&self, keep: &TransferKey) -> Option<TransferKey> {
        self.transfers
            .iter()
            .filter(|(k, _)| *k != keep)
            .max_by_key(|(_, t)| t.bytes)
            .map(|(k, _)| k.clone())
    }

    /// Makes room for `incoming` more bytes in `key`'s transfer: first within the peer's budget by
    /// dropping that peer's oldest other transfers, then within the global budget by dropping the
    /// largest other transfers. Returns false if there is still no room.
    fn make_room(&mut self, key: &TransferKey, incoming: usize) -> bool {
        let peer = key.0.clone();
        while self.peer_bytes(&peer) + incoming > self.limits.per_peer_bytes {
            match self.oldest_other(key, |k| k.0 == peer) {
                Some(victim) => {
                    tracing::debug!(
                        "[Reassembler::make_room] per-peer budget: dropping transfer tx_id={} from {}",
                        victim.2,
                        victim.0
                    );
                    self.transfers.remove(&victim);
                }
                None => return false,
            }
        }
        while self.bytes_in_use() + incoming > self.limits.global_bytes {
            match self.largest_other(key) {
                Some(victim) => {
                    tracing::debug!(
                        "[Reassembler::make_room] global budget: dropping transfer tx_id={} from {}",
                        victim.2,
                        victim.0
                    );
                    self.transfers.remove(&victim);
                }
                None => return false,
            }
        }
        true
    }

    /// Accepts one fragment. The caller handles fragments of transfers it has already completed
    /// (re-send `ACK_COMPLETE`) before calling this.
    pub fn accept(
        &mut self,
        key: TransferKey,
        header: &FragHeader,
        payload: &[u8],
        now: Instant,
    ) -> FragOutcome {
        let expired = self.expire(now);
        if expired > 0 {
            tracing::debug!(
                "[Reassembler::accept] expired {} partial transfer(s)",
                expired
            );
        }

        if let Err(reason) = self.validate_header(header, payload.len()) {
            let dropped_transfer = self.transfers.remove(&key).is_some();
            return FragOutcome::Rejected {
                reason,
                dropped_transfer,
            };
        }

        if let Some(existing) = self.transfers.get(&key)
            && (existing.frag_count != header.frag_count || existing.block_len != header.block_len)
        {
            let reason = format!(
                "metadata changed mid-transfer (frag_count {} -> {}, block_len {} -> {})",
                existing.frag_count, header.frag_count, existing.block_len, header.block_len
            );
            self.transfers.remove(&key);
            return FragOutcome::Rejected {
                reason,
                dropped_transfer: true,
            };
        }

        if !self.transfers.contains_key(&key) {
            let peer = key.0.clone();
            while self.transfers.keys().filter(|k| k.0 == peer).count()
                >= self.limits.max_transfers_per_peer
            {
                match self.oldest_other(&key, |k| k.0 == peer) {
                    Some(victim) => {
                        tracing::debug!(
                            "[Reassembler::accept] transfer limit: dropping transfer tx_id={} from {}",
                            victim.2,
                            victim.0
                        );
                        self.transfers.remove(&victim);
                    }
                    None => break,
                }
            }
            if self.limits.max_transfers_per_peer == 0 {
                return FragOutcome::Rejected {
                    reason: "no transfers allowed per peer".to_string(),
                    dropped_transfer: false,
                };
            }
            self.transfers
                .insert(key.clone(), Transfer::new(header, now));
        }

        let region = header.frag_index / ACK_WINDOW_SPAN as u16;
        let duplicate = match self.transfers.get_mut(&key) {
            Some(transfer) => transfer.fragments.contains_key(&header.frag_index),
            None => {
                return FragOutcome::Rejected {
                    reason: "transfer state missing".to_string(),
                    dropped_transfer: false,
                };
            }
        };

        if duplicate {
            // The sender is retransmitting, so our acknowledgement was lost or late: answer now.
            return match self.transfers.get_mut(&key) {
                Some(transfer) => {
                    transfer.dirty_regions.insert(region);
                    FragOutcome::Progress {
                        acks: transfer.take_acks(now),
                    }
                }
                None => FragOutcome::Rejected {
                    reason: "transfer state missing".to_string(),
                    dropped_transfer: false,
                },
            };
        }

        let length_check = match self.transfers.get_mut(&key) {
            Some(transfer) => transfer.check_length(header.frag_index, payload.len()),
            None => Err("transfer state missing".to_string()),
        };
        if let Err(reason) = length_check {
            self.transfers.remove(&key);
            return FragOutcome::Rejected {
                reason,
                dropped_transfer: true,
            };
        }

        if !self.make_room(&key, payload.len()) {
            self.transfers.remove(&key);
            return FragOutcome::Rejected {
                reason: format!(
                    "reassembly budget exhausted (per peer {}, global {})",
                    self.limits.per_peer_bytes, self.limits.global_bytes
                ),
                dropped_transfer: true,
            };
        }

        let complete = match self.transfers.get_mut(&key) {
            Some(transfer) => {
                transfer
                    .fragments
                    .insert(header.frag_index, payload.to_vec());
                transfer.bytes += payload.len();
                transfer.fragments.len() == transfer.frag_count as usize
            }
            None => {
                return FragOutcome::Rejected {
                    reason: "transfer state missing".to_string(),
                    dropped_transfer: false,
                };
            }
        };

        if complete {
            return match self.transfers.remove(&key) {
                Some(transfer) => FragOutcome::Complete {
                    block: transfer.assemble(),
                },
                None => FragOutcome::Rejected {
                    reason: "transfer state missing".to_string(),
                    dropped_transfer: false,
                },
            };
        }

        match self.transfers.get_mut(&key) {
            Some(transfer) => {
                transfer.dirty_regions.insert(region);
                transfer.new_since_ack = transfer.new_since_ack.saturating_add(1);
                let acks = if transfer.ack_due(now) {
                    transfer.take_acks(now)
                } else {
                    Vec::new()
                };
                FragOutcome::Progress { acks }
            }
            None => FragOutcome::Rejected {
                reason: "transfer state missing".to_string(),
                dropped_transfer: false,
            },
        }
    }
}
