use bingle_core::api::bingle_api::{
    BingleApi, BingleApiInternal, BingleError, Handle, OnConnectHandler, OnListeningHandler,
    OnMessageHandler, ProgressCallback, StartOptions, UserId,
};
use bingle_core::api::network_endpoint::NetworkEndpoint;
use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct MockBingleApi {
    pub progress_steps: Vec<u8>,
    pub on_listening: Mutex<Option<Arc<OnListeningHandler>>>,
    // Number of initial send_message_to_handle calls to fail with a transient error before
    // succeeding. 0 (default) = always succeed. Used to drive the drain-loop failure_reason path.
    pub send_fail_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl BingleApiInternal for MockBingleApi {
    fn get_relay_state(&self) -> String {
        "off".to_string()
    }
    fn notify_listening(&self, listening: bool, nat_type: bingle_core::engine::NatType) {
        if let Ok(guard) = self.on_listening.lock() {
            if let Some(handler) = guard.as_ref() {
                handler(listening, nat_type);
            }
        }
    }
}

impl BingleApi for MockBingleApi {
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> {
        Some("test-id".to_string())
    }
    fn get_user_id(&self) -> Option<String> {
        Some("test-id".to_string())
    }
    fn get_handle(&self) -> Option<String> {
        Some("testuser".to_string())
    }
    fn get_app_id(&self) -> Option<u64> {
        None
    }
    fn get_algo_provider_config(
        &self,
    ) -> Option<bingle_core::blockchain::algo_ops::AlgoChainConfig> {
        None
    }
    fn start(&self, _: &StartOptions) -> Result<(), BingleError> {
        Ok(())
    }
    fn stop(&self) {}
    fn network_change(&self) {}

    fn list_all_relays(&self, _: bool) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        vec![]
    }
    fn handle_lookup(&self, _: &Handle) -> Result<Option<UserId>, BingleError> {
        Ok(Some("test-id".to_string()))
    }
    fn handle_lookup_partial(&self, _: &Handle) -> Result<Option<(UserId, Handle)>, BingleError> {
        Ok(Some(("test-id".to_string(), "Test_User".to_string())))
    }
    fn handle_lookup_by_id(&self, _: &UserId) -> Option<Handle> {
        Some("testuser".to_string())
    }

    fn send_message_to_handle(
        &self,
        _handle: &Handle,
        _payload: serde_json::Value,
        progress_callback: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        use std::sync::atomic::Ordering;
        // Simulate an unreachable peer for the first `send_fail_count` attempts (transient), then
        // deliver. Lets a test observe the failure_reason being set and later cleared (#43).
        if self.send_fail_count.load(Ordering::SeqCst) > 0 {
            self.send_fail_count.fetch_sub(1, Ordering::SeqCst);
            return Err(BingleError::Retryable(
                "no ACK_COMPLETE received after 3 retries".to_string(),
            ));
        }
        if let Some(cb) = progress_callback {
            for &step in &self.progress_steps {
                cb(step, format!("Step {}%", step));
            }
        }
        Ok(true)
    }

    fn send_message_to_id(
        &self,
        _: &UserId,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_network(
        &self,
        _: &NetworkEndpoint,
        _: &UserId,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }

    fn send_message_to_id_with_response(
        &self,
        _: &UserId,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        Ok(serde_json::json!({}))
    }
    fn send_message_to_handle_with_response(
        &self,
        _: &Handle,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        Ok(serde_json::json!({}))
    }
    fn send_message_to_network_with_response(
        &self,
        _: &NetworkEndpoint,
        _: &UserId,
        _: serde_json::Value,
        _: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        Ok(serde_json::json!({}))
    }

    fn set_on_message(&self, _: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&self, _: Option<Arc<OnConnectHandler>>) {}
    fn set_on_listening(&self, handler: Option<Arc<OnListeningHandler>>) {
        if let Ok(mut guard) = self.on_listening.lock() {
            *guard = handler;
        }
    }
}

#[test]
fn test_handle_lookup_partial_maps_canonical_handle() {
    // Verifies the JSI layer forwards a partial lookup to the backing BingleApi and maps
    // the (id, canonical_handle) tuple into a HandleLookupPartialResult record.
    let mock_api = Arc::new(MockBingleApi {
        progress_steps: vec![],
        on_listening: Mutex::new(None),
        send_fail_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    });

    let jsi = BingleJsiApiImpl::init_for_tests(mock_api, None);

    let result = jsi
        .handle_lookup_partial("test".to_string())
        .expect("partial lookup should succeed");

    assert_eq!(result.id, "test-id");
    assert_eq!(result.canonical_handle, "Test_User");
}

#[test]
fn test_message_queue_with_mock_progress() {
    let mock_api = Arc::new(MockBingleApi {
        progress_steps: vec![10, 50, 90],
        on_listening: Mutex::new(None),
        send_fail_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    });

    let local_api: Arc<Mutex<Box<dyn BingleLocalApi>>> = Arc::new(Mutex::new(Box::new(
        BingleApiLocalImpl::new(LocalApiConfig::default()),
    )));

    let jsi = BingleJsiApiImpl::init_for_tests(mock_api, Some(local_api.clone()));

    // 1. Manually add a message and make it pending
    let timestamp = 999i64;
    {
        let mut guard = local_api.lock().unwrap();
        guard
            .add_message(
                "testuser".to_string(),
                vec!["recipient".to_string()],
                timestamp,
                "Hello".to_string(),
                None,
            )
            .unwrap();
        guard
            .update_message_status(timestamp, 0.0, None, None)
            .unwrap();
    }

    // 2. Start the JSI (starts background loop)
    jsi.start().unwrap();

    // 3. Simulate listening state
    jsi.api_for_tests()
        .notify_listening(true, bingle_core::engine::NatType::Restricted);

    // 4. Wait for processing loop. It sleeps for 5s, so we need some time.
    // We expect progress to go through 0.1, 0.5, 0.9 and finally 1.0.

    let mut reached_0_5 = false;
    let mut reached_1_0 = false;

    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(1000));
        let msgs = jsi.get_messages().unwrap();
        if let Some(msg) = msgs.iter().find(|m| m.timestamp == timestamp) {
            if msg.progress.map_or(false, |p| p >= 0.5) {
                reached_0_5 = true;
            }
            if msg.progress == Some(1.0) {
                reached_1_0 = true;
                break;
            }
        }
    }

    assert!(reached_0_5, "Message never reached 50% progress");
    assert!(reached_1_0, "Message never reached 100% progress");

    jsi.stop().unwrap();
}

// #43: a queued message whose delivery attempt fails (unreachable peer) must stay pending AND
// gain a concise, human-readable failure_reason, which then clears once delivery succeeds.
#[test]
fn queued_message_gains_failure_reason_then_clears_on_success() {
    use std::sync::atomic::AtomicUsize;

    // Fail the first delivery attempt (transient), then succeed on the retry.
    let mock_api = Arc::new(MockBingleApi {
        progress_steps: vec![],
        on_listening: Mutex::new(None),
        send_fail_count: Arc::new(AtomicUsize::new(1)),
    });

    let local_api: Arc<Mutex<Box<dyn BingleLocalApi>>> = Arc::new(Mutex::new(Box::new(
        BingleApiLocalImpl::new(LocalApiConfig::default()),
    )));
    let jsi = BingleJsiApiImpl::init_for_tests(mock_api, Some(local_api.clone()));

    let timestamp = 424343i64;
    {
        let mut guard = local_api.lock().unwrap();
        guard
            .add_message(
                "testuser".to_string(),
                vec!["recipient".to_string()],
                timestamp,
                "Hello".to_string(),
                None,
            )
            .unwrap();
        guard
            .update_message_status(timestamp, 0.0, None, None)
            .unwrap();
    }

    jsi.start().unwrap();
    jsi.api_for_tests()
        .notify_listening(true, bingle_core::engine::NatType::Restricted);

    // Phase 1: the first attempt fails transiently -> message stays pending and gains the
    // human-readable failure_reason (not the raw internal error).
    let mut saw_failure_reason = false;
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(500));
        let msgs = jsi.get_messages().unwrap();
        if let Some(m) = msgs.iter().find(|m| m.timestamp == timestamp)
            && let Some(reason) = &m.failure_reason
        {
            assert!(
                m.progress.map_or(true, |p| p < 1.0),
                "message must stay pending while it is still being retried"
            );
            assert_eq!(reason, "Recipient unreachable — will keep retrying");
            // The typed failure cause is surfaced alongside the reason (issue #99): a transient
            // connectivity failure maps to PeerUnreachable, and its retryability is derived via the
            // helper rather than stored per message.
            assert_eq!(
                m.failure_kind,
                Some(bingle_jsi::api::types::FailureKind::PeerUnreachable),
                "transient send failure should surface a PeerUnreachable kind"
            );
            assert!(
                bingle_jsi::api::types::failure_kind_is_retryable(
                    bingle_jsi::api::types::FailureKind::PeerUnreachable
                ),
                "a transient failure must be derivable as retryable"
            );
            saw_failure_reason = true;
            break;
        }
    }
    assert!(
        saw_failure_reason,
        "a queued message should gain a failure_reason after a failed delivery attempt"
    );

    // Phase 2: a later retry succeeds -> delivered (progress 1.0) and failure_reason cleared.
    let mut cleared = false;
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(500));
        let msgs = jsi.get_messages().unwrap();
        if let Some(m) = msgs.iter().find(|m| m.timestamp == timestamp)
            && m.progress == Some(1.0)
        {
            assert!(
                m.failure_reason.is_none(),
                "failure_reason must clear once delivery succeeds"
            );
            assert!(
                m.failure_kind.is_none(),
                "the typed failure cause must clear once delivery succeeds"
            );
            cleared = true;
            break;
        }
    }
    assert!(
        cleared,
        "message should eventually deliver and clear its failure_reason"
    );

    jsi.stop().unwrap();
}
