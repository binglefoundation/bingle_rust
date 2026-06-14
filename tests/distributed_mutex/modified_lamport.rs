use std::collections::HashMap;
use std::sync::{Arc, Mutex, Barrier, atomic::{AtomicIsize, Ordering}};
use std::thread;
use std::time::Duration;

use rust_comms::distributed_mutex::{DistributedMutex};
use rust_comms::distributed_mutex::ModifiedLamportDistributedMutex;
use rust_comms::messages::types::{MutexRequest, MutexResponse, MutexRelease};

struct TestNetwork {
    nodes: Arc<Mutex<HashMap<String, Arc<ModifiedLamportDistributedMutex>>>>,
    down: Arc<Mutex<Vec<String>>>,
}

impl TestNetwork {
    fn new() -> Self {
        Self { nodes: Arc::new(Mutex::new(HashMap::new())), down: Arc::new(Mutex::new(vec![])) }
    }

    #[allow(dead_code)]
    fn is_down(&self, id: &str) -> bool {
        let d = self.down.lock().expect("down lock");
        d.iter().any(|x| x == id)
    }

    fn drop_node(&self, id: &str) { self.down.lock().expect("down lock").push(id.to_string()); }

    fn add_node(&self, id: &str, all_ids: Vec<String>) -> Arc<ModifiedLamportDistributedMutex> {
        let _nodes_map = self.nodes.clone();
        let self_id = id.to_string();
        let net_for_req = self.nodes.clone();
        let net_for_rep = self.nodes.clone();
        let net_for_rel = self.nodes.clone();
        let down_ref = self.down.clone();
        let down_ref2 = self.down.clone();
        let down_ref3 = self.down.clone();

        let self_id_for_req = self_id.clone();
        let send_request = move |dest_id: &str, req: &MutexRequest| {
            // Check if destination is down, but do not hold the lock during handler call
            {
                let down = down_ref.lock().expect("down");
                if down.iter().any(|x| x == dest_id) { return; }
            }
            let dest_opt = {
                let map = net_for_req.lock().expect("net");
                map.get(dest_id).cloned()
            };
            if let Some(dest) = dest_opt {
                dest.handle_request(&self_id_for_req, req);
            }
        };

        let self_id_for_rep = self_id.clone();
        let send_reply = move |dest_id: &str, resp: &MutexResponse| {
            // Avoid holding any shared locks while delivering
            {
                let down = down_ref2.lock().expect("down");
                if down.iter().any(|x| x == dest_id) { return; }
            }
            let dest_opt = {
                let map = net_for_rep.lock().expect("net");
                map.get(dest_id).cloned()
            };
            if let Some(dest) = dest_opt {
                dest.handle_reply(&self_id_for_rep, resp);
            }
        };

        let self_id_for_rel = self_id.clone();
        let send_release = move |dest_id: &str, rel: &MutexRelease| {
            // Avoid holding any shared locks while delivering
            {
                let down = down_ref3.lock().expect("down");
                if down.iter().any(|x| x == dest_id) { return; }
            }
            let dest_opt = {
                let map = net_for_rel.lock().expect("net");
                map.get(dest_id).cloned()
            };
            if let Some(dest) = dest_opt {
                dest.handle_release(&self_id_for_rel, rel);
            }
        };

        let m = Arc::new(ModifiedLamportDistributedMutex::new(self_id.clone(), all_ids, send_request, send_reply, send_release));
        self.nodes.lock().expect("nodes").insert(self_id, m.clone());
        m
    }
}

#[ntest::timeout(30000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn modified_lamport_mutual_exclusion_3_nodes() {
    let net = TestNetwork::new();
    let ids = vec!["A".to_string(), "B".to_string(), "C".to_string()];

    let a = net.add_node("A", ids.clone());
    let b = net.add_node("B", ids.clone());
    let _c = net.add_node("C", ids.clone());

    let barrier = Arc::new(Barrier::new(2));
    let in_cs = Arc::new(AtomicIsize::new(0));

    let barrier1 = barrier.clone();
    let in_cs1 = in_cs.clone();
    let t1 = thread::spawn(move || {
        barrier1.wait();
        a.acquire(|| {
            let prev = in_cs1.fetch_add(1, Ordering::SeqCst);
            assert_eq!(prev, 0, "Two threads entered critical section concurrently (A)");
            // simulate work
            thread::sleep(Duration::from_millis(50));
            let prev2 = in_cs1.fetch_sub(1, Ordering::SeqCst);
            assert_eq!(prev2, 1);
            1u32
        })
    });

    let barrier2 = barrier.clone();
    let in_cs2 = in_cs.clone();
    let t2 = thread::spawn(move || {
        barrier2.wait();
        b.acquire(|| {
            let prev = in_cs2.fetch_add(1, Ordering::SeqCst);
            assert_eq!(prev, 0, "Two threads entered critical section concurrently (B)");
            thread::sleep(Duration::from_millis(50));
            let prev2 = in_cs2.fetch_sub(1, Ordering::SeqCst);
            assert_eq!(prev2, 1);
            2u32
        })
    });

    let r1 = t1.join().expect("t1");
    let r2 = t2.join().expect("t2");
    assert!(r1 == 1 || r2 == 2); // both ran
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn modified_lamport_majority_with_one_down() {
    let net = TestNetwork::new();
    let ids = vec!["A".to_string(), "B".to_string(), "C".to_string()];

    let a = net.add_node("A", ids.clone());
    let b = net.add_node("B", ids.clone());
    let c = net.add_node("C", ids.clone());

    // Drop C from network (no deliveries to C)
    net.drop_node("C");

    let in_cs = Arc::new(AtomicIsize::new(0));

    let in_cs_a = in_cs.clone();
    let ta = thread::spawn(move || {
        a.acquire(|| {
            let prev = in_cs_a.fetch_add(1, Ordering::SeqCst);
            assert_eq!(prev, 0, "A should enter alone");
            thread::sleep(Duration::from_millis(20));
            let prev2 = in_cs_a.fetch_sub(1, Ordering::SeqCst);
            assert_eq!(prev2, 1);
        });
    });

    ta.join().expect("a finished");

    // Ensure B can still acquire after A released
    let in_cs_b = in_cs.clone();
    let tb = thread::spawn(move || {
        b.acquire(|| {
            let prev = in_cs_b.fetch_add(1, Ordering::SeqCst);
            assert_eq!(prev, 0, "B should enter alone after A release");
            thread::sleep(Duration::from_millis(10));
            let prev2 = in_cs_b.fetch_sub(1, Ordering::SeqCst);
            assert_eq!(prev2, 1);
        });
    });

    tb.join().expect("b finished");

    // Avoid unused variable warning for c
    let _ = c;
}
