use rust_comms::dtls::dtls_openssl::openssl_impl::{PeerCmd, spawn_peer_worker};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[test]
fn peer_worker_receives_send_commands_in_order() {
    let seen = Arc::new(Mutex::new(Vec::<PeerCmd>::new()));
    let seen_for_worker = seen.clone();
    let peer = spawn_peer_worker("stage1-order", "test-handle-order", move |cmd| {
        let mut guard = seen_for_worker
            .lock()
            .expect("worker log mutex should lock");
        guard.push(cmd.clone());
        cmd != PeerCmd::Stop
    })
    .expect("peer worker should spawn");

    peer.send(PeerCmd::Send(vec![1, 2, 3]))
        .expect("send command should enqueue");
    peer.send(PeerCmd::Send(vec![9, 8]))
        .expect("second send command should enqueue");
    peer.send(PeerCmd::Stop)
        .expect("stop command should enqueue");

    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        let done = {
            let guard = seen.lock().expect("test log mutex should lock");
            guard.len() == 3
        };
        if done {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not process commands in time"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let guard = seen.lock().expect("test log mutex should lock");
    assert_eq!(
        *guard,
        vec![
            PeerCmd::Send(vec![1, 2, 3]),
            PeerCmd::Send(vec![9, 8]),
            PeerCmd::Stop,
        ]
    );
}

#[test]
fn peer_worker_stops_and_rejects_late_commands() {
    let seen = Arc::new(Mutex::new(0usize));
    let seen_for_worker = seen.clone();
    let peer = spawn_peer_worker("stage1-stop", "test-handle-stop", move |cmd| {
        let mut guard = seen_for_worker
            .lock()
            .expect("worker counter mutex should lock");
        *guard += 1;
        cmd != PeerCmd::Stop
    })
    .expect("peer worker should spawn");

    peer.send(PeerCmd::Stop)
        .expect("stop command should enqueue");

    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        let done = {
            let guard = seen.lock().expect("test counter mutex should lock");
            *guard == 1
        };
        if done {
            break;
        }
        assert!(Instant::now() < deadline, "worker did not stop in time");
        std::thread::sleep(Duration::from_millis(10));
    }

    let send_fail_deadline = Instant::now() + Duration::from_millis(500);
    let err = loop {
        match peer.send(PeerCmd::Send(vec![7])) {
            Ok(()) => {
                assert!(
                    Instant::now() < send_fail_deadline,
                    "late command unexpectedly kept succeeding"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(err) => break err,
        }
    };
    assert!(
        err.contains("peer command send failed"),
        "unexpected error text: {err}"
    );
}
