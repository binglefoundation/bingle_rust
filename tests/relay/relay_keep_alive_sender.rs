// Tests for RelayKeepAliveSender: the background loop that periodically sends
// Relay::KeepAlive to the registered relay to refresh the NAT mapping.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_comms::api::bingle_api::{
    BingleApiBoth, BingleError, NetworkEndpoint, ProgressCallback, UserId,
};
use rust_comms::relay::relay_keep_alive::RelayKeepAliveSender;

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

#[derive(Clone)]
struct SentMessage {
    nsk: NetworkEndpoint,
    user_id: String,
    message: serde_json::Value,
}

struct CapturingApi {
    sent: Arc<Mutex<Vec<SentMessage>>>,
}

impl InnerBingleApi for CapturingApi {
    fn send_message_to_network(
        &self,
        nsk: &NetworkEndpoint,
        user_id: &UserId,
        message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        if let Ok(mut v) = self.sent.lock() {
            v.push(SentMessage {
                nsk: nsk.clone(),
                user_id: user_id.clone(),
                message,
            });
        }
        Ok(true)
    }
}

/// Build a capturing mock api, returning the strong Arc (kept alive by the caller)
/// and the log of captured sends.
fn capturing_api() -> (Arc<dyn BingleApiBoth>, Arc<Mutex<Vec<SentMessage>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mock = MockApiBoth::new_with_api_override(Arc::new(CapturingApi { sent: sent.clone() }));
    let arc: Arc<dyn BingleApiBoth> = Arc::new(mock);
    (arc, sent)
}

fn sent_count(sent: &Arc<Mutex<Vec<SentMessage>>>) -> usize {
    sent.lock().map(|v| v.len()).unwrap_or(0)
}

/// Poll until `cond` holds or `timeout` elapses; returns whether the condition held.
fn wait_for(timeout: Duration, cond: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    cond()
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn sends_keep_alive_periodically_with_correct_payload() {
    let (api, sent) = capturing_api();
    let relay_addr = addr(19001);
    let mut sender = RelayKeepAliveSender::new(
        Arc::downgrade(&api),
        "RELAY_ID_1".to_string(),
        relay_addr,
        Duration::from_millis(30),
    );
    sender.start();

    assert!(
        wait_for(Duration::from_secs(5), || sent_count(&sent) >= 2),
        "expected at least 2 keep-alives, got {}",
        sent_count(&sent)
    );
    sender.stop();

    let messages = sent.lock().unwrap();
    for m in messages.iter() {
        assert_eq!(m.nsk, NetworkEndpoint::new_direct(relay_addr));
        assert_eq!(m.user_id, "RELAY_ID_1");
        assert_eq!(
            m.message.get("type").and_then(|v| v.as_str()),
            Some("KeepAlive")
        );
        assert_eq!(m.message.get("app"), Some(&serde_json::Value::Null));
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn does_not_send_before_first_interval() {
    let (api, sent) = capturing_api();
    let mut sender = RelayKeepAliveSender::new(
        Arc::downgrade(&api),
        "RELAY_ID_1".to_string(),
        addr(19002),
        Duration::from_secs(600),
    );
    sender.start();

    // Registration just refreshed the mapping, so the first keep-alive must wait
    // a full interval; with a 600s interval nothing may be sent here.
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        sent_count(&sent),
        0,
        "keep-alive sent before the first interval elapsed"
    );
    sender.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn stop_halts_sends_and_returns_promptly() {
    let (api, sent) = capturing_api();
    let interval = Duration::from_millis(30);
    let mut sender = RelayKeepAliveSender::new(
        Arc::downgrade(&api),
        "RELAY_ID_1".to_string(),
        addr(19003),
        interval,
    );
    sender.start();
    assert!(wait_for(Duration::from_secs(5), || sent_count(&sent) >= 1));

    let started = Instant::now();
    sender.stop();
    assert!(
        started.elapsed() < interval,
        "stop() must interrupt the wait, took {:?}",
        started.elapsed()
    );

    let count_after_stop = sent_count(&sent);
    std::thread::sleep(interval * 3);
    assert_eq!(
        sent_count(&sent),
        count_after_stop,
        "keep-alives sent after stop()"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn dropped_api_exits_loop_cleanly() {
    let (api, sent) = capturing_api();
    let mut sender = RelayKeepAliveSender::new(
        Arc::downgrade(&api),
        "RELAY_ID_1".to_string(),
        addr(19004),
        Duration::from_millis(20),
    );
    drop(api);
    sender.start();

    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        sent_count(&sent),
        0,
        "no sends possible once the api is gone"
    );
    // stop() joins the (already exited) thread and must not hang or panic
    sender.stop();
}
