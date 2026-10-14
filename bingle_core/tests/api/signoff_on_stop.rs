// Issue #279: a client that registered an advert in the DDB signs off when it stops, within a
// bounded time, and a relay or a node that never registered does not.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::{
    BingleApi, BingleApiInternal, BingleError, NetworkEndpoint, StartOptions,
};
use bingle_core::api::bingle_api_impl::{BingleApiImpl, SIGNOFF_TIMEOUT};
use bingle_core::ddb::DdbClient;

struct SignoffCountingDdb {
    signoffs: Arc<AtomicUsize>,
    signoff_delay: Duration,
}

impl DdbClient for SignoffCountingDdb {
    fn register_ip(&self, _endpoint: SocketAddr, _am_relay: bool) -> Result<(), BingleError> {
        Ok(())
    }
    fn register_relay(
        &self,
        _relay_id: String,
        _relay_sig: Option<String>,
    ) -> Result<(), BingleError> {
        Ok(())
    }
    fn lookup(&self, _id: &str) -> Result<NetworkEndpoint, BingleError> {
        Err(BingleError::Other("not found".to_string()))
    }
    fn signoff(&self) -> Result<(), BingleError> {
        std::thread::sleep(self.signoff_delay);
        self.signoffs.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn start_load_from_peer(&self, _peer_id: &str) -> Result<usize, BingleError> {
        Ok(0)
    }
}

fn api_with_counting_ddb(
    am_relay: bool,
    signoff_delay: Duration,
) -> (Arc<BingleApiImpl>, Arc<AtomicUsize>) {
    let mut opts = StartOptions::new("".into());
    opts.am_relay = am_relay;
    let api = BingleApiImpl::new(&opts);
    let signoffs = Arc::new(AtomicUsize::new(0));
    api.engine_set_ddb_client_for_tests(Arc::new(SignoffCountingDdb {
        signoffs: signoffs.clone(),
        signoff_delay,
    }));
    (api, signoffs)
}

fn endpoint() -> SocketAddr {
    "127.0.0.1:40000".parse().unwrap()
}

#[test]
#[cfg(not(target_os = "ios"))]
fn test_stop_signs_off_after_register_ip() {
    let (api, signoffs) = api_with_counting_ddb(false, Duration::ZERO);
    api.ddb_register_ip(endpoint(), false).unwrap();
    api.stop();
    assert_eq!(signoffs.load(Ordering::SeqCst), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn test_stop_signs_off_after_register_relay() {
    let (api, signoffs) = api_with_counting_ddb(false, Duration::ZERO);
    api.ddb_register_relay("RELAY".to_string(), None).unwrap();
    api.stop();
    assert_eq!(signoffs.load(Ordering::SeqCst), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn test_second_stop_does_not_sign_off_again() {
    let (api, signoffs) = api_with_counting_ddb(false, Duration::ZERO);
    api.ddb_register_ip(endpoint(), false).unwrap();
    api.stop();
    api.stop();
    drop(api);
    assert_eq!(signoffs.load(Ordering::SeqCst), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn test_stop_without_registration_does_not_sign_off() {
    let (api, signoffs) = api_with_counting_ddb(false, Duration::ZERO);
    api.stop();
    assert_eq!(signoffs.load(Ordering::SeqCst), 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn test_relay_stop_does_not_sign_off() {
    let (api, signoffs) = api_with_counting_ddb(true, Duration::ZERO);
    api.ddb_register_ip(endpoint(), true).unwrap();
    api.stop();
    assert_eq!(signoffs.load(Ordering::SeqCst), 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn test_stop_is_bounded_when_signoff_hangs() {
    let (api, _signoffs) = api_with_counting_ddb(false, Duration::from_secs(10));
    api.ddb_register_ip(endpoint(), false).unwrap();
    let started = Instant::now();
    api.stop();
    let took = started.elapsed();
    assert!(
        took < SIGNOFF_TIMEOUT + Duration::from_secs(2),
        "stop() took {:?} with a hanging sign-off",
        took
    );
}

/// Records each DDB call in order, to check no advert refresh follows the sign-off (#279 with #280).
struct EventLogDdb {
    events: Arc<std::sync::Mutex<Vec<&'static str>>>,
}

impl DdbClient for EventLogDdb {
    fn register_ip(&self, _endpoint: SocketAddr, _am_relay: bool) -> Result<(), BingleError> {
        self.events.lock().unwrap().push("register");
        Ok(())
    }
    fn register_relay(
        &self,
        _relay_id: String,
        _relay_sig: Option<String>,
    ) -> Result<(), BingleError> {
        self.events.lock().unwrap().push("register");
        Ok(())
    }
    fn lookup(&self, _id: &str) -> Result<NetworkEndpoint, BingleError> {
        Err(BingleError::Other("not found".to_string()))
    }
    fn signoff(&self) -> Result<(), BingleError> {
        self.events.lock().unwrap().push("signoff");
        Ok(())
    }
    fn start_load_from_peer(&self, _peer_id: &str) -> Result<usize, BingleError> {
        Ok(0)
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn test_stop_halts_advert_refresh_before_signing_off() {
    let mut opts = StartOptions::new("".into());
    // Refresh every 50ms, so several passes run before the stop.
    opts.advert_ttl = Some(Duration::from_millis(150));
    let api = BingleApiImpl::new(&opts);
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    api.engine_set_ddb_client_for_tests(Arc::new(EventLogDdb {
        events: events.clone(),
    }));
    api.ddb_register_ip(endpoint(), false).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    api.stop();
    std::thread::sleep(Duration::from_millis(300));

    let events = events.lock().unwrap().clone();
    let refreshes = events.iter().filter(|e| **e == "register").count();
    assert!(
        refreshes >= 2,
        "expected the advert to be refreshed before the stop, events: {:?}",
        events
    );
    assert_eq!(
        events.last(),
        Some(&"signoff"),
        "the sign-off must be the last DDB call, events: {:?}",
        events
    );
    assert_eq!(
        events.iter().filter(|e| **e == "signoff").count(),
        1,
        "events: {:?}",
        events
    );
}
