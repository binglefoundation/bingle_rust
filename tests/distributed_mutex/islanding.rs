use std::sync::{Arc, atomic::{AtomicBool, AtomicIsize, Ordering}};
use std::thread;
use std::time::Duration;

use rust_comms::distributed_mutex::DistributedMutex;
pub mod common;
use common::TestNetwork;
use crate::util::test_util::init_test_logging;

#[ntest::timeout(60000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn modified_lamport_partitioned_networks_no_dual_hold_c_and_d() {
    init_test_logging();

    // Networks:
    // a: A, B
    // b: A, B
    // c: A, B, C
    // d: A, B, D
    let net = TestNetwork::new();
    net.add_node("A");
    net.add_node("B");
    let a = net.create_mutex("A", vec!["A".to_string(), "B".to_string()]);
    let b = net.create_mutex("B", vec!["A".to_string(), "B".to_string()]);

    let in_cs = Arc::new(AtomicIsize::new(0));
    let violation = Arc::new(AtomicBool::new(false));

    // A acquires and holds briefly.
    let in_cs_a = in_cs.clone();
    let ta = thread::spawn(move || {
        a.acquire(|| {
            let prev = in_cs_a.fetch_add(1, Ordering::SeqCst);
            if prev != 0 { panic!("A should enter alone"); }
            thread::sleep(Duration::from_millis(40));
            let prev2 = in_cs_a.fetch_sub(1, Ordering::SeqCst);
            if prev2 != 1 { panic!("A should exit from 1"); }
        });
    });

    ta.join().expect("A finished");

    // B acquires and holds briefly.
    let in_cs_b = in_cs.clone();
    let tb = thread::spawn(move || {
        b.acquire(|| {
            let prev = in_cs_b.fetch_add(1, Ordering::SeqCst);
            if prev != 0 { panic!("B should enter alone"); }
            thread::sleep(Duration::from_millis(40));
            let prev2 = in_cs_b.fetch_sub(1, Ordering::SeqCst);
            if prev2 != 1 { panic!("B should exit from 1"); }
        });
    });

    tb.join().expect("B finished");

    // C & D attempt acquire simultaneously.
    net.add_node("C");
    let c = net.create_mutex("C", vec!["A".to_string(), "B".to_string(), "C".to_string()]);
    net.add_node("D");
    let d = net.create_mutex("D", vec!["A".to_string(), "B".to_string(), "D".to_string()]);

    let in_cs_c = in_cs.clone();
    let violation_c = violation.clone();
    let tc = thread::spawn(move || {
        c.acquire(|| {
            let prev = in_cs_c.fetch_add(1, Ordering::SeqCst);
            if prev != 0 { violation_c.store(true, Ordering::SeqCst); }
            thread::sleep(Duration::from_millis(500));
            let _ = in_cs_c.fetch_sub(1, Ordering::SeqCst);
        });
    });

    let in_cs_d = in_cs.clone();
    let violation_d = violation.clone();
    let td = thread::spawn(move || {
        d.acquire(|| {
            let prev = in_cs_d.fetch_add(1, Ordering::SeqCst);
            if prev != 0 { violation_d.store(true, Ordering::SeqCst); }
            thread::sleep(Duration::from_millis(500));
            let _ = in_cs_d.fetch_sub(1, Ordering::SeqCst);
        });
    });

    tc.join().expect("C finished");
    td.join().expect("D finished");

    assert!(
        !violation.load(Ordering::SeqCst),
        "C and D overlapped in the critical section"
    );
}