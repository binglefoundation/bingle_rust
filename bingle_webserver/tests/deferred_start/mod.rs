use std::sync::{Arc, Mutex};

use bingle_webserver::{AppState, try_start_api};
use bingle_local::api::bingle_local_api::{
    BingleLocalApi, Contact, ContactSource, Keypair, KeypairStatus, Message,
};
use rust_comms::api::bingle_api::{BingleError, StartOptions};
use rust_comms::blockchain::algo_ops::AlgoOps;
use crate::common::{TrackingMockBingleApi, CapturingMockBingleApi};

/// A test-only BingleLocalApi that lets us control keypair_status results.
struct ControllableLocalApi {
    keypair: Option<Keypair>,
    status_override: String,
}

impl ControllableLocalApi {
    fn new(status: &str) -> Self {
        Self {
            keypair: None,
            status_override: status.to_string(),
        }
    }
}

impl BingleLocalApi for ControllableLocalApi {
    fn generate_keypair(&mut self) -> Result<Keypair, BingleError> {
        let kp = Keypair { id: "TEST_ID".into(), passphrase: "TEST_PASS".into() };
        self.keypair = Some(kp.clone());
        Ok(kp)
    }

    fn register_keypair(&self, _handle: String) -> Result<bool, BingleError> { Ok(true) }

    fn get_algo_ops(&self) -> Result<AlgoOps, BingleError> {
        Err(BingleError::Other("not configured".to_string()))
    }

    fn add_contact(&mut self, _handle: String, _id: String, _source: ContactSource) -> Result<(), BingleError> { Ok(()) }
    fn block_contact(&mut self, _id: String) -> Result<(), BingleError> { Ok(()) }
    fn remove_contact(&mut self, _id: String) -> Result<(), BingleError> { Ok(()) }
    fn is_blocked(&self, _id: &str) -> Result<bool, BingleError> { Ok(false) }
    fn get_contacts(&self) -> Result<Vec<Contact>, BingleError> { Ok(Vec::new()) }

    fn add_message(
        &mut self,
        _sender_handle: String,
        _recipient_handles: Vec<String>,
        _timestamp: i64,
        _text: String,
        _cipher_suite: Option<String>,
    ) -> Result<(), BingleError> { Ok(()) }

    fn get_messages(&self) -> Result<Vec<Message>, BingleError> { Ok(Vec::new()) }
    fn save(&self, _path: &str) -> Result<(), BingleError> { Ok(()) }
    fn load(&mut self, _path: &str) -> Result<(), BingleError> { Ok(()) }

    fn keypair_status(&self) -> Result<KeypairStatus, BingleError> {
        Ok(KeypairStatus {
            status: self.status_override.clone(),
            id: self.keypair.as_ref().map(|k| k.id.clone()),
            handle: if self.status_override == "ACTIVE" { Some("test_handle".to_string()) } else { None },
            required_algo: None,
        })
    }

    fn get_keypair(&self) -> Result<Option<Keypair>, BingleError> {
        Ok(self.keypair.clone())
    }
}

#[test]
fn test_try_start_api_does_not_start_when_no_start_opts() {
    let started_flag = Arc::new(Mutex::new(false));
    let api = TrackingMockBingleApi::new(started_flag.clone());
    let local_api = ControllableLocalApi::new("ACTIVE");

    let state = AppState {
        api: Arc::new(api),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(Box::new(local_api) as Box<dyn BingleLocalApi>))),
        local_file: None,
        start_opts: None, // no deferred start configured
        api_started: Arc::new(Mutex::new(true)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };

    try_start_api(&state);
    // Should not attempt start since start_opts is None (already started)
    assert!(!*started_flag.lock().unwrap());
}

#[test]
fn test_try_start_api_does_not_start_when_keypair_not_active() {
    let started_flag = Arc::new(Mutex::new(false));
    let api = TrackingMockBingleApi::new(started_flag.clone());
    let local_api = ControllableLocalApi::new("None");

    let state = AppState {
        api: Arc::new(api),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(Box::new(local_api) as Box<dyn BingleLocalApi>))),
        local_file: None,
        start_opts: Some(StartOptions::default()),
        api_started: Arc::new(Mutex::new(false)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };

    try_start_api(&state);
    // Should not start because keypair status is "None"
    assert!(!*started_flag.lock().unwrap());
    assert!(!*state.api_started.lock().unwrap());
}

#[test]
fn test_try_start_api_does_not_start_when_keypair_unfunded() {
    let started_flag = Arc::new(Mutex::new(false));
    let api = TrackingMockBingleApi::new(started_flag.clone());
    let local_api = ControllableLocalApi::new("UNFUNDED");

    let state = AppState {
        api: Arc::new(api),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(Box::new(local_api) as Box<dyn BingleLocalApi>))),
        local_file: None,
        start_opts: Some(StartOptions::default()),
        api_started: Arc::new(Mutex::new(false)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };

    try_start_api(&state);
    assert!(!*started_flag.lock().unwrap());
    assert!(!*state.api_started.lock().unwrap());
}

#[test]
fn test_try_start_api_does_not_start_when_keypair_funded() {
    let started_flag = Arc::new(Mutex::new(false));
    let api = TrackingMockBingleApi::new(started_flag.clone());
    let local_api = ControllableLocalApi::new("FUNDED");

    let state = AppState {
        api: Arc::new(api),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(Box::new(local_api) as Box<dyn BingleLocalApi>))),
        local_file: None,
        start_opts: Some(StartOptions::default()),
        api_started: Arc::new(Mutex::new(false)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };

    try_start_api(&state);
    assert!(!*started_flag.lock().unwrap());
    assert!(!*state.api_started.lock().unwrap());
}

#[test]
fn test_try_start_api_starts_when_keypair_active() {
    let started_flag = Arc::new(Mutex::new(false));
    let api = TrackingMockBingleApi::new(started_flag.clone());
    let local_api = ControllableLocalApi::new("ACTIVE");

    let state = AppState {
        api: Arc::new(api),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(Box::new(local_api) as Box<dyn BingleLocalApi>))),
        local_file: None,
        start_opts: Some(StartOptions::default()),
        api_started: Arc::new(Mutex::new(false)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };

    try_start_api(&state);
    // Should have started because keypair is ACTIVE
    assert!(*started_flag.lock().unwrap());
    assert!(*state.api_started.lock().unwrap());
}

#[test]
fn test_try_start_api_does_not_start_twice() {
    let started_flag = Arc::new(Mutex::new(false));
    let api = TrackingMockBingleApi::new(started_flag.clone());
    let local_api = ControllableLocalApi::new("ACTIVE");

    let state = AppState {
        api: Arc::new(api),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(Box::new(local_api) as Box<dyn BingleLocalApi>))),
        local_file: None,
        start_opts: Some(StartOptions::default()),
        api_started: Arc::new(Mutex::new(true)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())), // already started
    };

    try_start_api(&state);
    // Should not call start() again since api_started is already true
    assert!(!*started_flag.lock().unwrap());
}

#[test]
fn test_try_start_api_sets_handle_and_passphrase_from_local_api() {
    let started_flag = Arc::new(Mutex::new(false));
    let captured_opts: Arc<Mutex<Option<StartOptions>>> = Arc::new(Mutex::new(None));
    let api = CapturingMockBingleApi::new(started_flag.clone(), captured_opts.clone());

    // Create a local API with a keypair already generated and status ACTIVE
    let mut local_api = ControllableLocalApi::new("ACTIVE");
    local_api.keypair = Some(Keypair {
        id: "ALGO_ADDRESS_123".to_string(),
        passphrase: "secret mnemonic phrase".to_string(),
    });

    // start_opts has empty handle and no passphrase (as parsed from CLI with --local)
    let base_opts = StartOptions::default(); // handle is "", algo_passphrase is None

    let state = AppState {
        api: Arc::new(api),
        messages: Arc::new(Mutex::new(Vec::new())),
        local_api: Some(Arc::new(Mutex::new(Box::new(local_api) as Box<dyn BingleLocalApi>))),
        local_file: None,
        start_opts: Some(base_opts),
        api_started: Arc::new(Mutex::new(false)),
        nat_type: Arc::new(Mutex::new("Unknown".to_string())),
    };

    try_start_api(&state);

    // API should have started
    assert!(*started_flag.lock().unwrap());

    // Verify the captured StartOptions have handle and passphrase from local API
    let opts = captured_opts.lock().unwrap();
    let opts = opts.as_ref().expect("start() should have been called with options");
    assert_eq!(opts.handle, "test_handle");
    assert_eq!(opts.algo_passphrase, Some("secret mnemonic phrase".to_string()));
}
