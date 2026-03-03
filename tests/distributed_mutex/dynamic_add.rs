use std::sync::{Arc, atomic::{AtomicIsize, Ordering}};
use std::thread;
use std::time::Duration;

use rust_comms::distributed_mutex::DistributedMutex;

mod common;
use common::TestNetwork;

#[ntest::timeout(30000)]
#[ignore]
#[test]
fn modified_lamport_dynamic_add_node_after_start() {
    crate::api::send_message_to_id_integration::init_test_logging();
    
    // Start with three nodes where only A and B are of interest; C exists but is idle.
    let net = TestNetwork::new();
    let ids = vec!["A".to_string(), "B".to_string(), "C".to_string()];
    for id in &ids { net.add_node(id); }

    let a = net.create_mutex("A", vec!["A".to_string(), "B".to_string()]);
    let b = net.create_mutex("B", vec!["A".to_string(), "B".to_string()]);
    let c = net.create_mutex("C", vec!["A".to_string(), "B".to_string(), "C".to_string()]);

    let in_cs = Arc::new(AtomicIsize::new(0));

    // First, A acquires and holds briefly.
    let in_cs_a = in_cs.clone();
    let ta = thread::spawn(move || {
        a.acquire(|| {
            let prev = in_cs_a.fetch_add(1, Ordering::SeqCst);
            assert_eq!(prev, 0, "A should enter alone");
            thread::sleep(Duration::from_millis(500));
            let prev2 = in_cs_a.fetch_sub(1, Ordering::SeqCst);
            assert_eq!(prev2, 1);
        });
    });

    // While A is in or after releasing, dynamically add Z (not in the original ids).
    // Z should be able to call acquire and eventually enter the critical section.
    let net2 = net.clone();
    let in_cs_z = in_cs.clone();
    let tz = thread::spawn(move || {
        // Give a small delay to ensure A has started/acquired.
        thread::sleep(Duration::from_millis(10));

        // Add Z with the extended view that includes itself.
        net2.add_node("Z");
        let z = net2.create_mutex("Z",vec!["A".to_string(), "B".to_string(), "C".to_string(), "Z".to_string()]);

        log::info!("Z added to network: z={:?}", z);

        // Now have Z try to acquire; it should eventually succeed.
        z.acquire(|| {
            let prev = in_cs_z.fetch_add(1, Ordering::SeqCst);
            assert_eq!(prev, 0, "Z should enter alone at its time");
            // brief work
            thread::sleep(Duration::from_millis(500));
            let prev2 = in_cs_z.fetch_sub(1, Ordering::SeqCst);
            assert_eq!(prev2, 1);
        });
    });

    ta.join().expect("A finished");
    tz.join().expect("Z finished");

    // Ensure B can still acquire after, demonstrating continued operation with the new node present.
    let in_cs_b = in_cs.clone();
    b.acquire(|| {
        let prev = in_cs_b.fetch_add(1, Ordering::SeqCst);
        assert_eq!(prev, 0, "B should enter alone after A and Z releases");
        thread::sleep(Duration::from_millis(500));
        let prev2 = in_cs_b.fetch_sub(1, Ordering::SeqCst);
        assert_eq!(prev2, 1);
    });
}
