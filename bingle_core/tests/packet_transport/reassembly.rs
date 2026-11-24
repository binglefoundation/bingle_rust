//! Receiver-side reassembly of `DATA_FRAG` transfers (spec §6.3–6.4, issue #518).

use bingle_core::api::bingle_api::{NetworkEndpoint, NetworkEndpointKey};
use bingle_core::packet_transport::frame::{FragHeader, PACKET_MAX};
use bingle_core::packet_transport::reassembly::{
    ACK_COALESCE, ACK_EVERY_FRAGMENTS, DEFAULT_PER_PEER_REASSEMBLY_BYTES, FragOutcome, Reassembler,
    ReassemblyLimits, TransferKey,
};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Fragment payload at the engine's current mtu of 1492 (`mtu - 12`).
const FRAG: usize = 1480;

fn peer(port: u16) -> NetworkEndpointKey {
    let addr: SocketAddr = format!("127.0.0.1:{port}")
        .parse()
        .expect("valid socket address");
    NetworkEndpoint::new_direct(addr)
        .get_key()
        .expect("a direct endpoint has a key")
}

fn key(port: u16, tx_id: u16) -> TransferKey {
    (peer(port), 0, tx_id)
}

/// A deterministic, non-repeating test block.
pub fn test_block(len: usize) -> Vec<u8> {
    let mut state: u32 = 0x1234_5678 ^ len as u32;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

/// Splits `block` into `DATA_FRAG` headers and payloads of `frag` bytes.
pub fn split(block: &[u8], frag: usize, tx_id: u16) -> Vec<(FragHeader, Vec<u8>)> {
    let frag_count = block.len().div_ceil(frag) as u16;
    block
        .chunks(frag)
        .enumerate()
        .map(|(i, chunk)| {
            (
                FragHeader {
                    tx_id,
                    frag_index: i as u16,
                    frag_count,
                    block_len: block.len() as u32,
                },
                chunk.to_vec(),
            )
        })
        .collect()
}

/// Block length that splits into exactly `count` fragments with a short last one.
fn len_for(count: usize) -> usize {
    (count - 1) * FRAG + 517
}

/// Feeds fragments in the given order; returns every completed block.
fn feed(
    r: &mut Reassembler,
    key: &TransferKey,
    frags: &[(FragHeader, Vec<u8>)],
    order: &[usize],
    now: Instant,
) -> Vec<Vec<u8>> {
    let mut completed = Vec::new();
    for &i in order {
        let (header, payload) = &frags[i];
        match r.accept(key.clone(), header, payload, now) {
            FragOutcome::Complete { block } => completed.push(block),
            FragOutcome::Progress { .. } => {}
            FragOutcome::Rejected { reason, .. } => panic!("fragment {i} rejected: {reason}"),
        }
    }
    completed
}

fn rejected(outcome: FragOutcome) -> (String, bool) {
    match outcome {
        FragOutcome::Rejected {
            reason,
            dropped_transfer,
        } => (reason, dropped_transfer),
        other => panic!("expected a rejection, got {other:?}"),
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn blocks_of_2_64_65_and_200_fragments_reassemble_exactly() {
    for count in [2usize, 64, 65, 200] {
        let mut r = Reassembler::new(ReassemblyLimits::standard());
        let block = test_block(len_for(count));
        let frags = split(&block, FRAG, 1);
        assert_eq!(frags.len(), count);
        let order: Vec<usize> = (0..count).collect();
        let completed = feed(&mut r, &key(1, 1), &frags, &order, Instant::now());
        assert_eq!(completed, vec![block], "{count} fragments");
        assert_eq!(r.transfer_count(), 0);
        assert_eq!(r.bytes_in_use(), 0);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_block_that_divides_evenly_reassembles_exactly() {
    let mut r = Reassembler::new(ReassemblyLimits::standard());
    let block = test_block(3 * FRAG);
    let frags = split(&block, FRAG, 2);
    let completed = feed(&mut r, &key(1, 2), &frags, &[2, 0, 1], Instant::now());
    assert_eq!(completed, vec![block]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn out_of_order_fragments_complete() {
    let mut r = Reassembler::new(ReassemblyLimits::standard());
    let block = test_block(len_for(200));
    let frags = split(&block, FRAG, 3);
    // Last fragment first, then a stride-7 permutation of the rest.
    let mut order = vec![199];
    order.extend((0..199).map(|i| (i * 7) % 199));
    let completed = feed(&mut r, &key(1, 3), &frags, &order, Instant::now());
    assert_eq!(completed, vec![block]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn duplicated_fragments_complete_once_and_are_acknowledged_at_once() {
    let mut r = Reassembler::new(ReassemblyLimits::standard());
    let block = test_block(len_for(65));
    let frags = split(&block, FRAG, 4);
    let now = Instant::now();
    let k = key(1, 4);
    let mut completed = Vec::new();
    for (header, payload) in &frags[..64] {
        for copy in 0..2 {
            match r.accept(k.clone(), header, payload, now) {
                FragOutcome::Progress { acks } if copy == 1 => {
                    assert!(!acks.is_empty(), "a duplicate is answered immediately")
                }
                FragOutcome::Progress { .. } => {}
                other => panic!("unexpected {other:?}"),
            }
        }
    }
    assert_eq!(
        r.bytes_in_use(),
        64 * FRAG,
        "duplicates are not stored twice"
    );
    if let FragOutcome::Complete { block } = r.accept(k.clone(), &frags[64].0, &frags[64].1, now) {
        completed.push(block);
    }
    assert_eq!(completed, vec![block]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_missing_fragment_retransmitted_later_completes_the_block() {
    let mut r = Reassembler::new(ReassemblyLimits::standard());
    let block = test_block(len_for(200));
    let frags = split(&block, FRAG, 5);
    let k = key(1, 5);
    let now = Instant::now();
    let without_37: Vec<usize> = (0..200).filter(|i| *i != 37).collect();
    assert!(feed(&mut r, &k, &frags, &without_37, now).is_empty());
    assert_eq!(r.transfer_count(), 1);
    let completed = feed(&mut r, &k, &frags, &[37], now + Duration::from_secs(2));
    assert_eq!(completed, vec![block]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn acknowledgements_are_coalesced() {
    let mut r = Reassembler::new(ReassemblyLimits::standard());
    let block = test_block(len_for(200));
    let frags = split(&block, FRAG, 6);
    let k = key(1, 6);
    let t0 = Instant::now();
    let acks = |outcome: FragOutcome| match outcome {
        FragOutcome::Progress { acks } => acks,
        other => panic!("unexpected {other:?}"),
    };

    // The first fragment is acknowledged at once.
    assert_eq!(
        acks(r.accept(k.clone(), &frags[0].0, &frags[0].1, t0)),
        vec![(0, 1)]
    );
    // The next ones are held back until ACK_EVERY_FRAGMENTS new fragments have arrived.
    let every = ACK_EVERY_FRAGMENTS as usize;
    for (header, payload) in &frags[1..every] {
        assert!(acks(r.accept(k.clone(), header, payload, t0)).is_empty());
    }
    let expected_bits = (1u64 << (every + 1)) - 1;
    assert_eq!(
        acks(r.accept(k.clone(), &frags[every].0, &frags[every].1, t0)),
        vec![(0, expected_bits)]
    );
    // After the coalescing interval the next fragment is acknowledged at once.
    let later = t0 + ACK_COALESCE;
    let next = every + 1;
    assert_eq!(
        acks(r.accept(k.clone(), &frags[next].0, &frags[next].1, later)),
        vec![(0, (1u64 << (next + 1)) - 1)]
    );
    // A fragment in the second region is acknowledged against base index 64.
    assert_eq!(
        acks(r.accept(k.clone(), &frags[70].0, &frags[70].1, later + ACK_COALESCE)),
        vec![(64, 1 << 6)]
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn inconsistent_metadata_is_rejected_without_panic() {
    let mut r = Reassembler::new(ReassemblyLimits::standard());
    let now = Instant::now();
    let payload = vec![0u8; FRAG];
    let header = |frag_index, frag_count, block_len| FragHeader {
        tx_id: 7,
        frag_index,
        frag_count,
        block_len,
    };

    // frag_index >= frag_count.
    rejected(r.accept(key(1, 7), &header(2, 2, 2000), &payload, now));
    // A single fragment is not a fragmented transfer.
    rejected(r.accept(key(1, 7), &header(0, 1, 1480), &payload, now));
    // block_len cannot fill frag_count fragments of this size, or is smaller than frag_count.
    rejected(r.accept(key(1, 7), &header(0, 2, 10_000), &payload, now));
    rejected(r.accept(key(1, 7), &header(0, 2, 1000), &payload[..10], now));
    rejected(r.accept(key(1, 7), &header(0, 100, 50), &payload[..1], now));
    // Claims over PACKET_MAX, and the largest values the fields can carry.
    let (reason, _) = rejected(r.accept(
        key(1, 7),
        &header(0, 60_000, PACKET_MAX as u32 + 1),
        &payload,
        now,
    ));
    assert!(reason.contains("PACKET_MAX"), "{reason}");
    rejected(r.accept(key(1, 7), &header(65_534, 65_535, u32::MAX), &payload, now));
    assert_eq!(r.transfer_count(), 0);
    assert_eq!(r.bytes_in_use(), 0);

    // Metadata that changes mid-transfer drops the transfer.
    let ok = r.accept(key(1, 8), &header(0, 3, 4000), &payload, now);
    assert!(matches!(ok, FragOutcome::Progress { .. }));
    let (_, dropped) = rejected(r.accept(key(1, 8), &header(1, 4, 4000), &payload, now));
    assert!(dropped);
    assert_eq!(r.transfer_count(), 0);

    // A middle fragment of a different length drops the transfer.
    assert!(matches!(
        r.accept(key(1, 9), &header(0, 3, 4000), &payload, now),
        FragOutcome::Progress { .. }
    ));
    let (_, dropped) = rejected(r.accept(key(1, 9), &header(1, 3, 4000), &payload[..1000], now));
    assert!(dropped);

    // A last fragment of the wrong length, seen after the regular length is known.
    assert!(matches!(
        r.accept(key(1, 10), &header(0, 3, 4000), &payload, now),
        FragOutcome::Progress { .. }
    ));
    let (_, dropped) = rejected(r.accept(key(1, 10), &header(2, 3, 4000), &payload[..100], now));
    assert!(dropped);

    // A last fragment seen first that a later regular fragment contradicts.
    assert!(matches!(
        r.accept(key(1, 11), &header(2, 3, 4000), &payload[..100], now),
        FragOutcome::Progress { .. }
    ));
    let (_, dropped) = rejected(r.accept(key(1, 11), &header(0, 3, 4000), &payload, now));
    assert!(dropped);
    assert_eq!(r.transfer_count(), 0);
    assert_eq!(r.bytes_in_use(), 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn partial_transfers_expire_after_the_timeout() {
    let mut r = Reassembler::new(ReassemblyLimits::standard());
    let block = test_block(len_for(10));
    let frags = split(&block, FRAG, 12);
    let t0 = Instant::now();
    feed(&mut r, &key(1, 12), &frags, &[0, 1, 2], t0);
    assert_eq!(r.expire(t0 + Duration::from_secs(29)), 0);
    assert_eq!(r.transfer_count(), 1);

    // Another transfer's fragment after the timeout sweeps the stale one.
    let other = split(&test_block(len_for(3)), FRAG, 13);
    feed(
        &mut r,
        &key(2, 13),
        &other,
        &[0],
        t0 + Duration::from_secs(30),
    );
    assert_eq!(r.transfer_count(), 1);
    assert_eq!(r.bytes_in_use(), FRAG);

    // The expired transfer's remaining fragments start again from nothing and cannot complete.
    let late: Vec<usize> = (3..10).collect();
    assert!(
        feed(
            &mut r,
            &key(1, 12),
            &frags,
            &late,
            t0 + Duration::from_secs(31)
        )
        .is_empty()
    );
}

fn small_limits() -> ReassemblyLimits {
    ReassemblyLimits {
        per_peer_bytes: 10 * FRAG,
        global_bytes: 25 * FRAG,
        max_transfers_per_peer: 8,
        timeout: Duration::from_secs(30),
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_per_peer_budget_drops_the_peers_oldest_transfer() {
    let mut r = Reassembler::new(small_limits());
    let now = Instant::now();
    let first = split(&test_block(9 * FRAG), FRAG, 20);
    let second = split(&test_block(9 * FRAG), FRAG, 21);
    let p = peer(1);

    feed(&mut r, &key(1, 20), &first, &[0, 1, 2, 3, 4], now);
    for i in 0..6 {
        feed(
            &mut r,
            &key(1, 21),
            &second,
            &[i],
            now + Duration::from_millis(1),
        );
        assert!(r.peer_bytes(&p) <= 10 * FRAG);
    }
    assert_eq!(r.transfer_count(), 1, "the older transfer was dropped");
    assert_eq!(r.peer_bytes(&p), 6 * FRAG);

    // A block larger than the per-peer budget is refused before anything is held.
    let too_big = split(&test_block(11 * FRAG), FRAG, 22);
    let (reason, _) = rejected(r.accept(key(1, 22), &too_big[0].0, &too_big[0].1, now));
    assert!(reason.contains("budget"), "{reason}");
    assert_eq!(r.peer_bytes(&p), 6 * FRAG);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_global_budget_drops_the_largest_other_transfer() {
    let mut r = Reassembler::new(small_limits());
    let now = Instant::now();
    let a = split(&test_block(10 * FRAG), FRAG, 30);
    let b = split(&test_block(10 * FRAG), FRAG, 31);
    let c = split(&test_block(10 * FRAG), FRAG, 32);

    feed(&mut r, &key(1, 30), &a, &(0..9).collect::<Vec<_>>(), now);
    feed(&mut r, &key(2, 31), &b, &(0..8).collect::<Vec<_>>(), now);
    for i in 0..9 {
        feed(&mut r, &key(3, 32), &c, &[i], now);
        assert!(r.bytes_in_use() <= 25 * FRAG);
    }
    assert_eq!(
        r.peer_bytes(&peer(1)),
        0,
        "the largest transfer was dropped"
    );
    assert_eq!(r.peer_bytes(&peer(2)), 8 * FRAG);
    assert_eq!(r.peer_bytes(&peer(3)), 9 * FRAG);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_transfer_limit_drops_the_peers_oldest_transfer() {
    let mut limits = ReassemblyLimits::standard();
    limits.max_transfers_per_peer = 2;
    let mut r = Reassembler::new(limits);
    let now = Instant::now();
    for tx in 0..3u16 {
        let frags = split(&test_block(len_for(3)), FRAG, tx);
        feed(
            &mut r,
            &key(1, tx),
            &frags,
            &[0],
            now + Duration::from_millis(tx as u64),
        );
    }
    assert_eq!(r.transfer_count(), 2);
    // The oldest (tx 0) is gone: its remaining fragments cannot complete it.
    let frags = split(&test_block(len_for(3)), FRAG, 0);
    assert!(feed(&mut r, &key(1, 0), &frags, &[1, 2], now).is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn announcing_a_large_block_allocates_nothing_up_front() {
    let mut r = Reassembler::new(ReassemblyLimits::standard());
    let block_len = DEFAULT_PER_PEER_REASSEMBLY_BYTES as u32;
    let frag_count = (block_len as usize).div_ceil(FRAG) as u16;
    let payload = vec![0u8; FRAG];
    let now = Instant::now();
    for tx in 0..8u16 {
        let header = FragHeader {
            tx_id: tx,
            frag_index: 5,
            frag_count,
            block_len,
        };
        assert!(matches!(
            r.accept(key(1, tx), &header, &payload, now),
            FragOutcome::Progress { .. }
        ));
    }
    // Eight transfers each announcing 16 MiB hold only the eight fragments that arrived.
    assert_eq!(r.transfer_count(), 8);
    assert_eq!(r.bytes_in_use(), 8 * FRAG);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn dropping_a_peer_releases_its_transfers() {
    let mut r = Reassembler::new(ReassemblyLimits::standard());
    let now = Instant::now();
    let frags = split(&test_block(len_for(4)), FRAG, 40);
    feed(&mut r, &key(1, 40), &frags, &[0, 1], now);
    feed(&mut r, &key(2, 40), &frags, &[0], now);
    r.drop_peer(&peer(1));
    assert_eq!(r.transfer_count(), 1);
    assert_eq!(r.bytes_in_use(), FRAG);
}
