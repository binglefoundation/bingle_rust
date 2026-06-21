use std::sync::Arc;

use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::callback::{ListeningCallback, LogCallback, MessageCallback};
use bingle_jsi::api::error::BingleJsiError;
use bingle_jsi::api::types::{
    BingleMessage, Contact, ContactSource, Keypair, KeypairStatusResponse,
    Message, NatTypeResponse, NetworkSourceKey, VersionInfo,
};

/// Stub implementation where every method returns NotImplemented.
/// Used to verify the trait is object-safe and all signatures are correct.
struct StubApi;

impl BingleJsiApi for StubApi {
    fn handle_lookup(&self, _handle: String) -> Result<String, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "handle_lookup".to_string(),
        })
    }

    fn send_message_to_id(
        &self,
        _user_id: String,
        _message: BingleMessage,
    ) -> Result<bool, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "send_message_to_id".to_string(),
        })
    }

    fn send_message_to_handle(
        &self,
        _handle: String,
        _message: BingleMessage,
    ) -> Result<bool, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "send_message_to_handle".to_string(),
        })
    }

    fn send_message_to_network(
        &self,
        _network_source_key: NetworkSourceKey,
        _user_id: String,
        _message: BingleMessage,
    ) -> Result<bool, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "send_message_to_network".to_string(),
        })
    }

    fn send_message_to_id_with_response(
        &self,
        _user_id: String,
        _message: BingleMessage,
    ) -> Result<BingleMessage, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "send_message_to_id_with_response".to_string(),
        })
    }

    fn send_message_to_handle_with_response(
        &self,
        _handle: String,
        _message: BingleMessage,
    ) -> Result<BingleMessage, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "send_message_to_handle_with_response".to_string(),
        })
    }

    fn send_message_to_network_with_response(
        &self,
        _network_source_key: NetworkSourceKey,
        _user_id: String,
        _message: BingleMessage,
    ) -> Result<BingleMessage, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "send_message_to_network_with_response".to_string(),
        })
    }

    fn queued(&self) -> Result<Vec<BingleMessage>, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "queued".to_string(),
        })
    }

    fn version(&self) -> Result<VersionInfo, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "version".to_string(),
        })
    }

    fn get_versions(&self) -> Result<std::collections::HashMap<String, VersionInfo>, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "get_versions".to_string(),
        })
    }

    fn get_nat_type(&self) -> Result<NatTypeResponse, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "get_nat_type".to_string(),
        })
    }

    fn generate_keypair(&self) -> Result<Keypair, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "generate_keypair".to_string(),
        })
    }

    fn register_keypair(&self, _handle: String) -> Result<(), BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "register_keypair".to_string(),
        })
    }

    fn add_contact(
        &self,
        _handle: String,
        _id: String,
        _source: ContactSource,
    ) -> Result<(), BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "add_contact".to_string(),
        })
    }

    fn block_contact(&self, _id: String) -> Result<(), BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "block_contact".to_string(),
        })
    }

    fn remove_contact(&self, _id: String) -> Result<(), BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "remove_contact".to_string(),
        })
    }

    fn is_blocked(&self, _id: String) -> Result<bool, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "is_blocked".to_string(),
        })
    }

    fn get_contacts(&self) -> Result<Vec<Contact>, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "get_contacts".to_string(),
        })
    }

    fn add_message(
        &self,
        _sender_handle: String,
        _recipient_handles: Vec<String>,
        _timestamp: i64,
        _text: String,
        _cipher_suite: Option<String>,
    ) -> Result<(), BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "add_message".to_string(),
        })
    }

    fn get_messages(&self) -> Result<Vec<Message>, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "get_messages".to_string(),
        })
    }

    fn queue_message(&self, _recipient_handles: Vec<String>, _text: String) -> Result<(), BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "queue_message".to_string(),
        })
    }

    fn keypair_status(&self) -> Result<KeypairStatusResponse, BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "keypair_status".to_string(),
        })
    }

    fn save(&self, _path: String) -> Result<(), BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "save".to_string(),
        })
    }

    fn load(&self, _path: String) -> Result<(), BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "load".to_string(),
        })
    }

    fn set_message_callback(&self, _callback: Box<dyn MessageCallback>) {
        // no-op stub
    }

    fn set_log_callback(&self, _callback: Box<dyn LogCallback>) {
        // no-op stub
    }

    fn set_listening_callback(&self, _callback: Box<dyn ListeningCallback>) {
        // no-op stub
    }

    fn start(&self) -> Result<(), BingleJsiError> {
        Err(BingleJsiError::NotImplemented {
            reason: "start".to_string(),
        })
    }

    fn is_started(&self) -> bool {
        false
    }
}

#[test]
fn trait_is_object_safe() {
    let api: Arc<dyn BingleJsiApi> = Arc::new(StubApi);
    let result = api.version();
    assert!(result.is_err());
}

#[test]
fn stub_handle_lookup_returns_not_implemented() {
    let api = StubApi;
    let result = api.handle_lookup("alice".to_string());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, BingleJsiError::NotImplemented { .. }));
}

#[test]
fn stub_send_message_to_id_returns_not_implemented() {
    let api = StubApi;
    let msg = BingleMessage {
        app: None,
        r#type: None,
        tag: None,
        response_tag: None,
        text: Some("hi".to_string()),
        data: None,
        cipher_suite: None,
    };
    let result = api.send_message_to_id("user1".to_string(), msg);
    assert!(result.is_err());
}

#[test]
fn stub_send_message_to_handle_returns_not_implemented() {
    let api = StubApi;
    let msg = BingleMessage {
        app: None,
        r#type: None,
        tag: None,
        response_tag: None,
        text: Some("hi".to_string()),
        data: None,
        cipher_suite: None,
    };
    let result = api.send_message_to_handle("alice".to_string(), msg);
    assert!(result.is_err());
}

#[test]
fn stub_queued_returns_not_implemented() {
    let api = StubApi;
    let result = api.queued();
    assert!(result.is_err());
}

#[test]
fn stub_generate_keypair_returns_not_implemented() {
    let api = StubApi;
    let result = api.generate_keypair();
    assert!(result.is_err());
}

#[test]
fn stub_get_contacts_returns_not_implemented() {
    let api = StubApi;
    let result = api.get_contacts();
    assert!(result.is_err());
}

#[test]
fn stub_get_messages_returns_not_implemented() {
    let api = StubApi;
    let result = api.get_messages();
    assert!(result.is_err());
}

#[test]
fn stub_save_returns_not_implemented() {
    let api = StubApi;
    let result = api.save("/tmp/state.json".to_string());
    assert!(result.is_err());
}

#[test]
fn stub_load_returns_not_implemented() {
    let api = StubApi;
    let result = api.load("/tmp/state.json".to_string());
    assert!(result.is_err());
}

#[test]
fn stub_keypair_status_returns_not_implemented() {
    let api = StubApi;
    let result = api.keypair_status();
    assert!(result.is_err());
}

#[test]
fn stub_is_blocked_returns_not_implemented() {
    let api = StubApi;
    let result = api.is_blocked("id1".to_string());
    assert!(result.is_err());
}

#[test]
fn stub_get_nat_type_returns_not_implemented() {
    let api = StubApi;
    let result = api.get_nat_type();
    assert!(result.is_err());
}

#[test]
fn stub_start_returns_not_implemented() {
    let api = StubApi;
    let result = api.start();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, BingleJsiError::NotImplemented { .. }));
}

#[test]
fn stub_is_started_returns_false() {
    let api = StubApi;
    assert!(!api.is_started());
}

#[test]
fn stub_set_log_callback_does_not_panic() {
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestLogCallback {
        called: Arc<AtomicBool>,
    }
    impl LogCallback for TestLogCallback {
        fn on_log(&self, _timestamp: i64, _level: String, _message: String) {
            self.called.store(true, Ordering::SeqCst);
        }
    }

    let api = StubApi;
    let called = Arc::new(AtomicBool::new(false));
    let cb = TestLogCallback { called: called.clone() };
    api.set_log_callback(Box::new(cb));
    // StubApi is a no-op, so called should remain false
    assert!(!called.load(Ordering::SeqCst));
}

#[test]
fn stub_set_message_callback_does_not_panic() {
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestMessageCallback {
        called: Arc<AtomicBool>,
    }
    impl MessageCallback for TestMessageCallback {
        fn on_message(&self, _sender_id: String, _sender_handle: String, _message: BingleMessage) {
            self.called.store(true, Ordering::SeqCst);
        }
    }

    let api = StubApi;
    let called = Arc::new(AtomicBool::new(false));
    let cb = TestMessageCallback { called: called.clone() };
    api.set_message_callback(Box::new(cb));
    // StubApi is a no-op, so called should remain false
    assert!(!called.load(Ordering::SeqCst));
}

#[test]
fn stub_set_listening_callback_does_not_panic() {
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestListeningCallback {
        called: Arc<AtomicBool>,
    }
    impl ListeningCallback for TestListeningCallback {
        fn on_listening(&self, _listening: bool, _nat_type: String) {
            self.called.store(true, Ordering::SeqCst);
        }
    }

    let api = StubApi;
    let called = Arc::new(AtomicBool::new(false));
    let cb = TestListeningCallback { called: called.clone() };
    api.set_listening_callback(Box::new(cb));
    // StubApi is a no-op, so called should remain false
    assert!(!called.load(Ordering::SeqCst));
}
