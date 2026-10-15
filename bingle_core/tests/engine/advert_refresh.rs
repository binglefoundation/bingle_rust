// Tests for the client advert refresh (issue #280): a node re-registers its last advert every
// third of the advert time to live, so relays never expire a live node's advert.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::{BingleApiInternal, BingleError, NetworkEndpoint, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::ddb::{AdvertRegistration, DdbClient};

/// Records every registration it is asked to make.
#[derive(Default)]
struct RecordingDdbClient {
    registrations: Mutex<Vec<AdvertRegistration>>,
    signoffs: Mutex<usize>,
}

impl RecordingDdbClient {
    fn count(&self) -> usize {
        self.registrations.lock().unwrap().len()
    }
    fn last(&self) -> Option<AdvertRegistration> {
        self.registrations.lock().unwrap().last().cloned()
    }
}

impl DdbClient for RecordingDdbClient {
    fn register_ip(&self, endpoint: SocketAddr, am_relay: bool) -> Result<(), BingleError> {
        self.registrations
            .lock()
            .unwrap()
            .push(AdvertRegistration::Ip { endpoint, am_relay });
        Ok(())
    }
    fn register_relay(
        &self,
        relay_id: String,
        relay_sig: Option<String>,
    ) -> Result<(), BingleError> {
        self.registrations
            .lock()
            .unwrap()
            .push(AdvertRegistration::Relay {
                relay_id,
                relay_sig,
            });
        Ok(())
    }
    fn lookup(&self, _id: &str) -> Result<NetworkEndpoint, BingleError> {
        Err(BingleError::Other("not used".to_string()))
    }
    fn signoff(&self) -> Result<(), BingleError> {
        *self.signoffs.lock().unwrap() += 1;
        Ok(())
    }
    fn start_load_from_peer(&self, _peer_id: &str) -> Result<usize, BingleError> {
        Ok(0)
    }
}

fn api_with_ttl(ttl: Duration) -> (Arc<BingleApiImpl>, Arc<RecordingDdbClient>) {
    let mut options = StartOptions::new("advert-refresh".into());
    options.advert_ttl = Some(ttl);
    let api = BingleApiImpl::new(&options);
    let ddb = Arc::new(RecordingDdbClient::default());
    api.engine_set_ddb_client_for_tests(ddb.clone());
    (api, ddb)
}

fn wait_for(cond: impl Fn() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    cond()
}

fn addr() -> SocketAddr {
    "203.0.113.5:4433".parse().unwrap()
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ttl_defaults_when_unset() {
    let api = BingleApiImpl::new(&StartOptions::new("advert-default".into()));
    assert_eq!(
        api.engine_for_tests().advert_ttl_for_tests(),
        Some(bingle_core::ddb::DEFAULT_ADVERT_TTL)
    );
}

/// A client's advert is re-registered while it is live, so it is never absent from a relay for
/// longer than one refresh interval.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn client_registration_is_repeated_every_third_of_the_ttl() {
    let (api, ddb) = api_with_ttl(Duration::from_millis(600));
    api.ddb_register_ip(addr(), false).unwrap();
    assert_eq!(ddb.count(), 1);
    assert!(api.engine_for_tests().advert_refresh_running_for_tests());

    assert!(
        wait_for(|| ddb.count() >= 3, Duration::from_secs(5)),
        "expected repeated registrations, got {}",
        ddb.count()
    );
    assert_eq!(
        ddb.last(),
        Some(AdvertRegistration::Ip {
            endpoint: addr(),
            am_relay: false
        })
    );
    api.engine_for_tests().stop_advert_refresh_for_tests();
}

/// After a relay switch the refresh repeats the new registration, not the first one.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn refresh_repeats_the_latest_registration() {
    let (api, ddb) = api_with_ttl(Duration::from_millis(600));
    api.ddb_register_relay("RELAY_A".to_string(), None).unwrap();
    api.ddb_register_relay("RELAY_B".to_string(), None).unwrap();
    let before = ddb.count();
    assert!(wait_for(|| ddb.count() > before, Duration::from_secs(5)));
    assert_eq!(
        ddb.last(),
        Some(AdvertRegistration::Relay {
            relay_id: "RELAY_B".to_string(),
            relay_sig: None
        })
    );
    api.engine_for_tests().stop_advert_refresh_for_tests();
}

/// Relays never expire relay adverts, so registering as a relay stops the refresh.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_registration_stops_the_refresh() {
    let (api, _ddb) = api_with_ttl(Duration::from_millis(600));
    api.ddb_register_ip(addr(), false).unwrap();
    assert!(api.engine_for_tests().advert_refresh_running_for_tests());
    api.ddb_register_ip(addr(), true).unwrap();
    assert!(!api.engine_for_tests().advert_refresh_running_for_tests());
}

/// A refresh after a sign-off would put the advert back, so signing off stops the refresh.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn signoff_stops_the_refresh() {
    let (api, ddb) = api_with_ttl(Duration::from_millis(600));
    api.ddb_register_ip(addr(), false).unwrap();
    api.ddb_signoff().unwrap();
    assert!(!api.engine_for_tests().advert_refresh_running_for_tests());
    let after_signoff = ddb.count();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(ddb.count(), after_signoff, "no refresh after the sign-off");
    assert_eq!(*ddb.signoffs.lock().unwrap(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn zero_ttl_disables_the_refresh() {
    let (api, _ddb) = api_with_ttl(Duration::ZERO);
    api.ddb_register_ip(addr(), false).unwrap();
    assert!(!api.engine_for_tests().advert_refresh_running_for_tests());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn failed_registration_does_not_start_the_refresh() {
    struct FailingDdbClient;
    impl DdbClient for FailingDdbClient {
        fn register_ip(&self, _e: SocketAddr, _r: bool) -> Result<(), BingleError> {
            Err(BingleError::Other("no relay".to_string()))
        }
        fn register_relay(&self, _id: String, _sig: Option<String>) -> Result<(), BingleError> {
            Err(BingleError::Other("no relay".to_string()))
        }
        fn lookup(&self, _id: &str) -> Result<NetworkEndpoint, BingleError> {
            Err(BingleError::Other("no relay".to_string()))
        }
        fn signoff(&self) -> Result<(), BingleError> {
            Ok(())
        }
        fn start_load_from_peer(&self, _peer_id: &str) -> Result<usize, BingleError> {
            Ok(0)
        }
    }
    let (api, _ddb) = api_with_ttl(Duration::from_millis(600));
    api.engine_set_ddb_client_for_tests(Arc::new(FailingDdbClient));
    assert!(api.ddb_register_ip(addr(), false).is_err());
    assert!(!api.engine_for_tests().advert_refresh_running_for_tests());
    assert_eq!(api.engine_for_tests().advert_registration_for_tests(), None);
}
