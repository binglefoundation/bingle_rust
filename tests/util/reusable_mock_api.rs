use std::sync::Arc;

use rust_comms::api::bingle_api::BingleApiBoth;

/// Delegating trait: mirrors `rust_comms::api::bingle_api::BingleApi` but provides defaults,
/// so test mocks can override only the methods they care about.
pub trait InnerBingleApi {
    fn set_on_listening(
        &self,
        _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>,
    ) {
    }

    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> {
        None
    }

    fn get_handle(&self) -> Option<String> {
        None
    }

    fn get_user_id(&self) -> Option<String> {
        None
    }

    fn debug_print_options(&self) {}

    fn get_my_id(&self) -> Option<String> {
        None
    }

    fn get_app_id(&self) -> Option<u64> {
        None
    }

    fn start(&self, _options: &rust_comms::api::bingle_api::StartOptions) -> Result<(), String> {
        Ok(())
    }

    fn stop(&self) {}

    fn network_change(&self) {}

    fn handle_lookup(&self, _handle: &rust_comms::api::bingle_api::Handle) -> Result<Option<rust_comms::api::bingle_api::UserId>, String> {
        Ok(None)
    }

    fn send_message_to_id(
        &self,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> bool {
        false
    }

    fn send_message_to_handle(
        &self,
        _handle: &rust_comms::api::bingle_api::Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> bool {
        false
    }

    fn send_message_to_network(
        &self,
        _network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> bool {
        false
    }

    fn send_message_to_id_with_response(
        &self,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, String> {
        Err("ni".into())
    }

    fn send_message_to_handle_with_response(
        &self,
        _handle: &rust_comms::api::bingle_api::Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, String> {
        Err("ni".into())
    }

    fn send_message_to_network_with_response(
        &self,
        _network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, String> {
        Err("ni".into())
    }

    fn set_on_message(&self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}

    fn set_on_connect(&self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

#[derive(Clone)]
pub struct MockApiBoth {
    inner_bingle_api: Arc<dyn InnerBingleApi + Send + Sync>,
    inner_bingle_api_internal: Arc<dyn InnerBingleApiInternal + Send + Sync>,
}

impl MockApiBoth {
    pub fn new() -> Self {
        struct DefaultApi;
        impl InnerBingleApi for DefaultApi {}

        struct DefaultInternal;
        impl InnerBingleApiInternal for DefaultInternal {}

        Self {
            inner_bingle_api: Arc::new(DefaultApi),
            inner_bingle_api_internal: Arc::new(DefaultInternal),
        }
    }

    pub fn new_with_api_override(
        inner_bingle_api: Arc<dyn InnerBingleApi + Send + Sync>,
    ) -> Self {
        struct DefaultInternal;
        impl InnerBingleApiInternal for DefaultInternal {}

        Self {
            inner_bingle_api,
            inner_bingle_api_internal: Arc::new(DefaultInternal),
        }
    }

    pub fn new_with_internal_override(
        inner_bingle_api_internal: Arc<dyn InnerBingleApiInternal + Send + Sync>,
    ) -> Self {
        struct DefaultApi;
        impl InnerBingleApi for DefaultApi {}

        Self {
            inner_bingle_api: Arc::new(DefaultApi),
            inner_bingle_api_internal,
        }
    }
}

impl rust_comms::api::bingle_api::BingleApi for MockApiBoth {
    fn set_on_listening(
        &mut self,
        handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>,
    ) {
        self.inner_bingle_api.set_on_listening(handler);
    }

    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> {
        self.inner_bingle_api
            .get_algo_provider_config()
    }

    fn get_handle(&self) -> Option<String> {
        self.inner_bingle_api.get_handle()
    }

    fn get_user_id(&self) -> Option<String> {
        self.inner_bingle_api.get_user_id()
    }

    fn debug_print_options(&self) {
        self.inner_bingle_api.debug_print_options();
    }

    fn get_my_id(&self) -> Option<String> {
        self.inner_bingle_api.get_my_id()
    }

    fn get_app_id(&self) -> Option<u64> {
        self.inner_bingle_api.get_app_id()
    }

    fn start(&mut self, options: &rust_comms::api::bingle_api::StartOptions) -> Result<(), String> {
        self.inner_bingle_api.start(options)
    }

    fn stop(&mut self) {
        self.inner_bingle_api.stop();
    }

    fn network_change(&mut self) {
        self.inner_bingle_api.network_change();
    }

    fn handle_lookup(&self, handle: &rust_comms::api::bingle_api::Handle) -> Result<Option<rust_comms::api::bingle_api::UserId>, String> {
        self.inner_bingle_api.handle_lookup(handle)
    }

    fn send_message_to_id(
        &self,
        user_id: &rust_comms::api::bingle_api::UserId,
        message: serde_json::Value,
        progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> bool {
        self.inner_bingle_api.send_message_to_id(user_id, message, progress)
    }

    fn send_message_to_handle(
        &self,
        handle: &rust_comms::api::bingle_api::Handle,
        message: serde_json::Value,
        progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> bool {
        self.inner_bingle_api.send_message_to_handle(handle, message, progress)
    }

    fn send_message_to_network(
        &self,
        network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint,
        user_id: &rust_comms::api::bingle_api::UserId,
        message: serde_json::Value,
        progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> bool {
        self.inner_bingle_api
            .send_message_to_network(network_source_key, user_id, message, progress)
    }

    fn send_message_to_id_with_response(
        &self,
        user_id: &rust_comms::api::bingle_api::UserId,
        message: serde_json::Value,
        progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, String> {
        self.inner_bingle_api
            .send_message_to_id_with_response(user_id, message, progress)
    }

    fn send_message_to_handle_with_response(
        &self,
        handle: &rust_comms::api::bingle_api::Handle,
        message: serde_json::Value,
        progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, String> {
        self.inner_bingle_api
            .send_message_to_handle_with_response(handle, message, progress)
    }

    fn send_message_to_network_with_response(
        &self,
        network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint,
        user_id: &rust_comms::api::bingle_api::UserId,
        message: serde_json::Value,
        progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, String> {
        self.inner_bingle_api
            .send_message_to_network_with_response(network_source_key, user_id, message, progress)
    }

    fn set_on_message(&mut self, handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {
        self.inner_bingle_api.set_on_message(handler);
    }

    fn set_on_connect(&mut self, handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {
        self.inner_bingle_api.set_on_connect(handler);
    }
}

impl rust_comms::api::bingle_api::BingleApiInternal for MockApiBoth {
    fn get_relay_state(&self) -> String {
        self.inner_bingle_api_internal.get_relay_state()
    }

    fn set_state(&self, state: rust_comms::engine::EngineState) {
        self.inner_bingle_api_internal.set_state(state);
    }

    fn get_state(&self) -> rust_comms::engine::EngineState {
        self.inner_bingle_api_internal.get_state()
    }

    fn set_nat_type(&self, nat: rust_comms::engine::NatType) {
        self.inner_bingle_api_internal.set_nat_type(nat);
    }

    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> {
        self.inner_bingle_api_internal.get_last_public_addr()
    }

    fn ddb_register_ip(&self, endpoint: std::net::SocketAddr, am_relay: bool) -> Result<(), String> {
        self.inner_bingle_api_internal.ddb_register_ip(endpoint, am_relay)
    }

    fn ddb_register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), String> {
        self.inner_bingle_api_internal.ddb_register_relay(relay_id, relay_sig)
    }

    fn update_turn_listener_relay(&self, relay_id: String, relay_addr: std::net::SocketAddr) -> Result<(), String> {
        self.inner_bingle_api_internal.update_turn_listener_relay(relay_id, relay_addr)
    }

    fn turn_client_handle_listen_response(&self, relay_addr: std::net::SocketAddr, relay_id: String) {
        self.inner_bingle_api_internal
            .turn_client_handle_listen_response(relay_addr, relay_id);
    }

    fn turn_lookup_addr_by_id(&self, id: String) -> Option<std::net::SocketAddr> {
        self.inner_bingle_api_internal.turn_lookup_addr_by_id(id)
    }

    fn turn_handle_call(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr) -> i32 {
        self.inner_bingle_api_internal.turn_handle_call(source, dest)
    }

    fn turn_handle_listen(&self, id: String, source: std::net::SocketAddr) -> bool {
        self.inner_bingle_api_internal.turn_handle_listen(id, source)
    }

    fn turn_handle_called(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr, channel: u16) {
        self.inner_bingle_api_internal.turn_handle_called(source, dest, channel);
    }

    fn notify_listening(&self, listening: bool) {
        self.inner_bingle_api_internal.notify_listening(listening);
    }

    fn set_relay_state(&self, state: rust_comms::engine::RelayState) {
        self.inner_bingle_api_internal.set_relay_state(state);
    }

    fn get_peer_ddb_target(&self) -> Option<usize> {
        self.inner_bingle_api_internal.get_peer_ddb_target()
    }

    fn ddb_upsert_record(&self, record: rust_comms::ddb::AdvertRecord) {
        self.inner_bingle_api_internal.ddb_upsert_record(record);
    }

    fn ddb_backend_size(&self) -> usize {
        self.inner_bingle_api_internal.ddb_backend_size()
    }

    fn initialize_relay(&self) {
        self.inner_bingle_api_internal.initialize_relay();
    }

    fn is_relay(&self) -> bool {
        self.inner_bingle_api_internal.is_relay()
    }

    fn signal_signon_complete(&self) {
        self.inner_bingle_api_internal.signal_signon_complete();
    }

    fn reset_signon_complete(&self) {
        self.inner_bingle_api_internal.reset_signon_complete();
    }

    fn ripple_message(&self, message: serde_json::Value, originator_id: String) {
        self.inner_bingle_api_internal.ripple_message(message, originator_id);
    }
}

/// Delegating trait: mirrors `rust_comms::api::bingle_api::BingleApiInternal` but provides defaults,
/// so test mocks can override only the methods they care about.
pub trait InnerBingleApiInternal {
    fn get_relay_state(&self) -> String {
        "off".to_string()
    }

    fn set_state(&self, _state: rust_comms::engine::EngineState) {}

    fn get_state(&self) -> rust_comms::engine::EngineState {
        rust_comms::engine::EngineState::StunIdentify
    }

    fn set_nat_type(&self, _nat: rust_comms::engine::NatType) {}

    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> {
        None
    }

    fn ddb_register_ip(&self, _endpoint: std::net::SocketAddr, _am_relay: bool) -> Result<(), String> {
        Err("ni".into())
    }

    fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> {
        Err("ni".into())
    }

    fn update_turn_listener_relay(&self, _relay_id: String, _relay_addr: std::net::SocketAddr) -> Result<(), String> {
        Err("ni".into())
    }

    fn turn_client_handle_listen_response(&self, _relay_addr: std::net::SocketAddr, _relay_id: String) {}

    fn turn_lookup_addr_by_id(&self, _id: String) -> Option<std::net::SocketAddr> {
        None
    }

    fn turn_handle_call(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr) -> i32 {
        -1
    }

    fn turn_handle_listen(&self, _id: String, _source: std::net::SocketAddr) -> bool {
        false
    }

    fn turn_handle_called(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr, _channel: u16) {}

    fn notify_listening(&self, _listening: bool) {}

    fn set_relay_state(&self, _state: rust_comms::engine::RelayState) {}

    fn get_peer_ddb_target(&self) -> Option<usize> {
        None
    }

    fn ddb_upsert_record(&self, _record: rust_comms::ddb::AdvertRecord) {}

    fn ddb_backend_size(&self) -> usize {
        0
    }

    fn initialize_relay(&self) {}

    fn is_relay(&self) -> bool {
        false
    }

    fn signal_signon_complete(&self) {}

    fn reset_signon_complete(&self) {}

    fn ripple_message(&self, _message: serde_json::Value, _originator_id: String) {}
}

/// Test helper: wrap a concrete `BingleApiBoth` into a leaked `Arc<Mutex<dyn BingleApiBoth>>` and return a `Weak`.
/// This mirrors the helper used elsewhere in tests, but is scoped under `crate::util::reusable_mock_api`.
pub fn to_weak_api_both<T: BingleApiBoth + 'static>(api: T) -> rust_comms::api::bingle_api::BingleApiBothType {
    let arc: Arc<dyn BingleApiBoth> = Arc::new(api);
    let weak = Arc::downgrade(&arc);

    // Leak the Arc to keep it alive for the duration of the test process.
    Box::leak(Box::new(arc));

    weak
}

pub fn to_weak<T: BingleApiBoth + 'static>(api: T) -> rust_comms::api::bingle_api::BingleApiBothType {
    to_weak_api_both(api)
}
