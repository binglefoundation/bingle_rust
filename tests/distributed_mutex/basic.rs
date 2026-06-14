use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;
use std::thread;

use rust_comms::distributed_mutex::{DistributedMutex, LocalDistributedMutex};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unit_acquire_returns_value() {
    let m = LocalDistributedMutex::new();
    let res = m.acquire(|| 2 + 3);
    assert_eq!(res, 5);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unit_exclusive_execution_across_threads() {
    let m = Arc::new(LocalDistributedMutex::new());
    let inside = Arc::new(AtomicUsize::new(0));
    let max_inside = Arc::new(AtomicUsize::new(0));

    let threads: Vec<_> = (0..8)
        .map(|_| {
            let m = Arc::clone(&m);
            let inside = Arc::clone(&inside);
            let max_inside = Arc::clone(&max_inside);
            thread::spawn(move || {
                for _ in 0..200 {
                    m.acquire(|| {
                        let prev = inside.fetch_add(1, Ordering::SeqCst);
                        // If mutual exclusion holds, there should be no one else inside.
                        assert_eq!(prev, 0, "more than one thread inside critical section");

                        // Give the scheduler a chance to switch threads while we hold the lock.
                        thread::sleep(Duration::from_micros(50));

                        // Track the peak concurrent entries we observed.
                        let current = inside.load(Ordering::SeqCst);
                        loop {
                            let recorded = max_inside.load(Ordering::SeqCst);
                            if current <= recorded { break; }
                            if max_inside
                                .compare_exchange(recorded, current, Ordering::SeqCst, Ordering::SeqCst)
                                .is_ok() { break; }
                        }

                        let prev2 = inside.fetch_sub(1, Ordering::SeqCst);
                        assert_eq!(prev2, 1, "critical section exit count mismatch");
                    });
                }
            })
        })
        .collect();

    for t in threads {
        t.join().expect("thread join");
    }

    assert_eq!(max_inside.load(Ordering::SeqCst), 1, "observed more than one concurrent entry");
}
