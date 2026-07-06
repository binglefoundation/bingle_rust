use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::util::reusable_mock_api::MockApiBoth;
use rust_comms::api::bingle_api::BingleApiBoth;
use rust_comms::messages::handlers::{FromStruct, MessageHandler};
use rust_comms::messages::router::Router;
use rust_comms::messages::types::PingPing;
use rust_comms::messages::{Message, PingMessage};

struct SlowPingHandler {
    started_tx: mpsc::Sender<()>,
    finished: Arc<Mutex<bool>>,
}

impl MessageHandler for SlowPingHandler {
    fn on_ping_ping(&self, _api: Arc<dyn BingleApiBoth>, _from: &FromStruct, _msg: &PingPing) {
        let _ = self.started_tx.send(());
        std::thread::sleep(Duration::from_millis(250));
        if let Ok(mut guard) = self.finished.lock() {
            *guard = true;
        }
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn route_with_network_runs_handler_on_background_thread() {
    let router = Arc::new(Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
    ));
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let finished = Arc::new(Mutex::new(false));

    let handler = SlowPingHandler {
        started_tx,
        finished: finished.clone(),
    };
    let message = Message::Ping(PingMessage::Ping(PingPing {
        app: "ping".to_string(),
        tag: None,
        response_tag: None,
        text: Some("background-thread-check".to_string()),
        data: None,
    }));
    let endpoint = "127.0.0.1:12000".parse().expect("endpoint should parse");
    let nsk = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(endpoint);

    let started = Instant::now();
    Router::with_current_router(router.clone(), || {
        router.route_with_network(handler, &message, "SENDERID", &nsk);
    });
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(100),
        "route_with_network should return quickly, elapsed: {:?}",
        elapsed
    );

    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("background handler should start");

    for _ in 0..20 {
        if let Ok(guard) = finished.lock() {
            if *guard {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    panic!("background handler should finish within timeout");
}
