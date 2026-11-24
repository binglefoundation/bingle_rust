//! Sender-side acknowledgement state for one fragmented send (spec §5.3).

use super::frame::ACK_WINDOW_SPAN;

/// Which fragments of a fragmented send the receiver has acknowledged.
#[derive(Debug)]
pub struct SendWindow {
    frag_count: u16,
    acked: Vec<u64>,
    acked_count: usize,
    lowest_unacked: usize,
    any_ack: bool,
    complete: bool,
    /// Bumped whenever new acknowledgement arrives, so a waiting sender can tell progress apart
    /// from a timeout.
    progress: u64,
}

impl SendWindow {
    pub fn new(frag_count: u16) -> Self {
        Self {
            frag_count,
            acked: vec![0; (frag_count as usize).div_ceil(64)],
            acked_count: 0,
            lowest_unacked: 0,
            any_ack: false,
            complete: false,
            progress: 0,
        }
    }

    pub fn frag_count(&self) -> u16 {
        self.frag_count
    }

    pub fn is_acked(&self, index: usize) -> bool {
        index < self.frag_count as usize && self.acked[index / 64] & (1u64 << (index % 64)) != 0
    }

    /// First fragment not yet acknowledged: the start of the in-flight window.
    pub fn lowest_unacked(&self) -> usize {
        self.lowest_unacked
    }

    /// One past the last fragment the in-flight window allows to be outstanding.
    pub fn window_end(&self) -> usize {
        (self.lowest_unacked + ACK_WINDOW_SPAN).min(self.frag_count as usize)
    }

    pub fn acked_count(&self) -> usize {
        self.acked_count
    }

    /// True once any acknowledgement (window or complete) has arrived for this send.
    pub fn any_ack(&self) -> bool {
        self.any_ack
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn progress(&self) -> u64 {
        self.progress
    }

    /// Applies an `ACK_WINDOW`. Bits past the last fragment are ignored. Returns true if it
    /// acknowledged anything new.
    pub fn apply_ack_window(&mut self, base_index: u16, ack_bits: u64) -> bool {
        self.any_ack = true;
        let mut changed = false;
        for bit in 0..64usize {
            if ack_bits & (1u64 << bit) == 0 {
                continue;
            }
            let index = base_index as usize + bit;
            if index >= self.frag_count as usize {
                break;
            }
            if !self.is_acked(index) {
                self.acked[index / 64] |= 1u64 << (index % 64);
                self.acked_count += 1;
                changed = true;
            }
        }
        while self.lowest_unacked < self.frag_count as usize && self.is_acked(self.lowest_unacked) {
            self.lowest_unacked += 1;
        }
        if changed {
            self.progress += 1;
        }
        changed
    }

    /// Records the terminal `ACK_COMPLETE`.
    pub fn mark_complete(&mut self) {
        self.any_ack = true;
        self.complete = true;
        self.progress += 1;
    }

    /// Fragments in `[lowest_unacked, sent_end)` not yet acknowledged: what a timeout retransmits.
    pub fn unacked_in_window(&self, sent_end: usize) -> Vec<usize> {
        let end = sent_end.min(self.window_end());
        (self.lowest_unacked..end)
            .filter(|i| !self.is_acked(*i))
            .collect()
    }
}
