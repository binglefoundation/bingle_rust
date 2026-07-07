use bingle_core::engine::BingleAccessUnsafeForTests;

use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::{BingleApi, OnListeningHandler, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};

// Minimal DTLS mock that starts/stops without real networking
#[derive(Clone)]
struct MockDtls;
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<bingle_core::dtls::UdpNetworkMux>) -> DtlsResult<()> {
        Ok(())
    }
    fn stop(&mut self) -> DtlsResult<()> {
        Ok(())
    }
    fn send(
        &self,
        _to: &bingle_core::api::bingle_api::NetworkEndpoint,
        _data: &[u8],
    ) -> DtlsResult<()> {
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        None
    }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
    fn set_handle_new_session(
        &mut self,
        _handler: Option<bingle_core::dtls::dtls_trait::HandleNewSession>,
    ) {
    }
    fn with_handle_message(self, _handler: HandleMessage) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> {
        None
    }
    fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_ca_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_cipher_suite(
        &self,
        _endpoint: &bingle_core::api::bingle_api::NetworkEndpoint,
    ) -> Option<String> {
        None
    }
    fn forget_peers(&self) {}
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn engine_static_ip_triggers_on_listening_handler() {
    // Prepare a temp sentinel path
    let dir = tempfile::tempdir().expect("tempdir");
    let sentinel_path = dir.path().join("engine_listening.sentinel");
    let sentinel_str = sentinel_path.to_string_lossy().to_string();

    // Build API with mock DTLS and install sentinel creator handler
    let api = BingleApiImpl::new_with_dtls(Box::new(MockDtls));
    let path_clone = sentinel_str.clone();
    let handler: Arc<OnListeningHandler> = Arc::new(
        move |listening: bool, _nat_type: bingle_core::engine::NatType| {
            if listening {
                if let Ok(mut f) = fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&path_clone)
                {
                    use std::io::Write;
                    let _ = writeln!(f, "listening");
                }
            } else {
                let _ = fs::remove_file(&path_clone);
            }
        },
    );
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.set_on_listening(Some(handler)));

    // Static IP with port 0 ensures start_with_addr path and ephemeral bind
    let opts = StartOptions {
        handle: "engine_user".to_string(),
        algo_passphrase: None,
        static_ip: Some("127.0.0.1:0".parse().unwrap()),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };

    // Start should cause Engine::start_with_addr to notify listening=true via EngineInternalPtr
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts))
        .expect("api.start");

    // Wait for sentinel to appear (up to 2s)
    let start = Instant::now();
    let timeout = Duration::from_secs(2);
    while !sentinel_path.exists() && start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        sentinel_path.exists(),
        "sentinel should be created by on_listening handler after static start"
    );

    // Stop should remove sentinel
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.stop());
    assert!(
        !sentinel_path.exists(),
        "sentinel should be removed on stop (listening=false)"
    );
}
