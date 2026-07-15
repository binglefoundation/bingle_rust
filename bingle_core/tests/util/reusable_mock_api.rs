use std::sync::Arc;

use bingle_core::api::bingle_api::BingleApiBoth;

/// Delegating trait: mirrors `bingle_core::api::bingle_api::BingleApi` but provides defaults,
/// so test mocks can override only the methods they care about.
pub trait InnerBingleApi {
    fn list_all_relays(
        &self,
        _include_self: bool,
    ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        Vec::new()
    }
    fn set_on_listening(
        &self,
        _handler: Option<std::sync::Arc<bingle_core::api::bingle_api::OnListeningHandler>>,
    ) {
    }

    fn get_algo_provider_config(
        &self,
    ) -> Option<bingle_core::blockchain::algo_ops::AlgoChainConfig> {
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

    fn start(
        &self,
        _options: &bingle_core::api::bingle_api::StartOptions,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        Ok(())
    }

    fn stop(&self) {}

    fn network_change(&self) {}

    fn handle_lookup(
        &self,
        _handle: &bingle_core::api::bingle_api::Handle,
    ) -> Result<
        Option<bingle_core::api::bingle_api::UserId>,
        bingle_core::api::bingle_api::BingleError,
    > {
        Ok(None)
    }

    fn handle_lookup_partial(
        &self,
        _handle: &bingle_core::api::bingle_api::Handle,
    ) -> Result<
        Option<(
            bingle_core::api::bingle_api::UserId,
            bingle_core::api::bingle_api::Handle,
        )>,
        bingle_core::api::bingle_api::BingleError,
    > {
        Ok(None)
    }

    fn handle_lookup_by_id(
        &self,
        _user_id: &bingle_core::api::bingle_api::UserId,
    ) -> Option<bingle_core::api::bingle_api::Handle> {
        None
    }

    fn send_message_to_id(
        &self,
        _user_id: &bingle_core::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(false)
    }

    fn send_message_to_handle(
        &self,
        _handle: &bingle_core::api::bingle_api::Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(false)
    }

    fn send_message_to_network(
        &self,
        _network_source_key: &bingle_core::api::bingle_api::NetworkEndpoint,
        _user_id: &bingle_core::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(false)
    }

    fn send_message_to_id_with_response(
        &self,
        _user_id: &bingle_core::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".into(),
        ))
    }

    fn send_message_to_handle_with_response(
        &self,
        _handle: &bingle_core::api::bingle_api::Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".into(),
        ))
    }

    fn send_message_to_network_with_response(
        &self,
        _network_source_key: &bingle_core::api::bingle_api::NetworkEndpoint,
        _user_id: &bingle_core::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".into(),
        ))
    }

    fn send_message_to_network_with_response_timeout(
        &self,
        network_source_key: &bingle_core::api::bingle_api::NetworkEndpoint,
        user_id: &bingle_core::api::bingle_api::UserId,
        message: serde_json::Value,
        progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
        _timeout: std::time::Duration,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        // Default mirrors the production trait: ignore the override and defer to
        // the untimed variant so existing mocks keep working unchanged.
        self.send_message_to_network_with_response(network_source_key, user_id, message, progress)
    }

    fn set_on_message(
        &self,
        _handler: Option<Arc<bingle_core::api::bingle_api::OnMessageHandler>>,
    ) {
    }

    fn set_on_connect(
        &self,
        _handler: Option<Arc<bingle_core::api::bingle_api::OnConnectHandler>>,
    ) {
    }

    fn get_signing_key(&self) -> Option<ed25519_dalek::SigningKey> {
        None
    }
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

    pub fn new_with_api_override(inner_bingle_api: Arc<dyn InnerBingleApi + Send + Sync>) -> Self {
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

    pub fn new_with_both_overrides(
        inner_bingle_api: Arc<dyn InnerBingleApi + Send + Sync>,
        inner_bingle_api_internal: Arc<dyn InnerBingleApiInternal + Send + Sync>,
    ) -> Self {
        Self {
            inner_bingle_api,
            inner_bingle_api_internal,
        }
    }
}

impl bingle_core::api::bingle_api::BingleApi for MockApiBoth {
    fn list_all_relays(
        &self,
        include_self: bool,
    ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        self.inner_bingle_api.list_all_relays(include_self)
    }
    fn set_on_listening(
        &self,
        handler: Option<std::sync::Arc<bingle_core::api::bingle_api::OnListeningHandler>>,
    ) {
        self.inner_bingle_api.set_on_listening(handler);
    }

    fn get_algo_provider_config(
        &self,
    ) -> Option<bingle_core::blockchain::algo_ops::AlgoChainConfig> {
        self.inner_bingle_api.get_algo_provider_config()
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

    fn start(
        &self,
        options: &bingle_core::api::bingle_api::StartOptions,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api.start(options)
    }

    fn stop(&self) {
        self.inner_bingle_api.stop();
    }

    fn network_change(&self) {
        self.inner_bingle_api.network_change();
    }

    fn handle_lookup(
        &self,
        handle: &bingle_core::api::bingle_api::Handle,
    ) -> Result<
        Option<bingle_core::api::bingle_api::UserId>,
        bingle_core::api::bingle_api::BingleError,
    > {
        self.inner_bingle_api.handle_lookup(handle)
    }

    fn handle_lookup_partial(
        &self,
        handle: &bingle_core::api::bingle_api::Handle,
    ) -> Result<
        Option<(
            bingle_core::api::bingle_api::UserId,
            bingle_core::api::bingle_api::Handle,
        )>,
        bingle_core::api::bingle_api::BingleError,
    > {
        self.inner_bingle_api.handle_lookup_partial(handle)
    }

    fn handle_lookup_by_id(
        &self,
        user_id: &bingle_core::api::bingle_api::UserId,
    ) -> Option<bingle_core::api::bingle_api::Handle> {
        self.inner_bingle_api.handle_lookup_by_id(user_id)
    }

    fn send_message_to_id(
        &self,
        user_id: &bingle_core::api::bingle_api::UserId,
        message: serde_json::Value,
        progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api
            .send_message_to_id(user_id, message, progress)
    }

    fn send_message_to_handle(
        &self,
        handle: &bingle_core::api::bingle_api::Handle,
        message: serde_json::Value,
        progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api
            .send_message_to_handle(handle, message, progress)
    }

    fn send_message_to_network(
        &self,
        network_source_key: &bingle_core::api::bingle_api::NetworkEndpoint,
        user_id: &bingle_core::api::bingle_api::UserId,
        message: serde_json::Value,
        progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api.send_message_to_network(
            network_source_key,
            user_id,
            message,
            progress,
        )
    }

    fn send_message_to_id_with_response(
        &self,
        user_id: &bingle_core::api::bingle_api::UserId,
        message: serde_json::Value,
        progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api
            .send_message_to_id_with_response(user_id, message, progress)
    }

    fn send_message_to_handle_with_response(
        &self,
        handle: &bingle_core::api::bingle_api::Handle,
        message: serde_json::Value,
        progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api
            .send_message_to_handle_with_response(handle, message, progress)
    }

    fn send_message_to_network_with_response(
        &self,
        network_source_key: &bingle_core::api::bingle_api::NetworkEndpoint,
        user_id: &bingle_core::api::bingle_api::UserId,
        message: serde_json::Value,
        progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api.send_message_to_network_with_response(
            network_source_key,
            user_id,
            message,
            progress,
        )
    }

    fn send_message_to_network_with_response_timeout(
        &self,
        network_source_key: &bingle_core::api::bingle_api::NetworkEndpoint,
        user_id: &bingle_core::api::bingle_api::UserId,
        message: serde_json::Value,
        progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api
            .send_message_to_network_with_response_timeout(
                network_source_key,
                user_id,
                message,
                progress,
                timeout,
            )
    }

    fn set_on_message(&self, handler: Option<Arc<bingle_core::api::bingle_api::OnMessageHandler>>) {
        self.inner_bingle_api.set_on_message(handler);
    }

    fn set_on_connect(&self, handler: Option<Arc<bingle_core::api::bingle_api::OnConnectHandler>>) {
        self.inner_bingle_api.set_on_connect(handler);
    }
}

impl bingle_core::api::bingle_api::BingleApiInternal for MockApiBoth {
    fn get_relay_state(&self) -> String {
        self.inner_bingle_api_internal.get_relay_state()
    }

    fn set_state(&self, state: bingle_core::engine::EngineState) {
        self.inner_bingle_api_internal.set_state(state);
    }

    fn get_state(&self) -> bingle_core::engine::EngineState {
        self.inner_bingle_api_internal.get_state()
    }

    fn set_nat_type(&self, nat: bingle_core::engine::NatType) {
        self.inner_bingle_api_internal.set_nat_type(nat);
    }

    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> {
        self.inner_bingle_api_internal.get_last_public_addr()
    }

    fn ddb_register_ip(
        &self,
        endpoint: std::net::SocketAddr,
        am_relay: bool,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api_internal
            .ddb_register_ip(endpoint, am_relay)
    }

    fn ddb_register_relay(
        &self,
        relay_id: String,
        relay_sig: Option<String>,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api_internal
            .ddb_register_relay(relay_id, relay_sig)
    }

    fn update_turn_listener_relay(
        &self,
        relay_id: String,
        relay_addr: std::net::SocketAddr,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        self.inner_bingle_api_internal
            .update_turn_listener_relay(relay_id, relay_addr)
    }

    fn turn_client_handle_listen_response(
        &self,
        relay_addr: std::net::SocketAddr,
        relay_id: String,
    ) {
        self.inner_bingle_api_internal
            .turn_client_handle_listen_response(relay_addr, relay_id);
    }

    fn turn_lookup_addr_by_id(&self, id: String) -> Option<std::net::SocketAddr> {
        self.inner_bingle_api_internal.turn_lookup_addr_by_id(id)
    }

    fn turn_handle_call(
        &self,
        source_id: String,
        dest_id: String,
        source: std::net::SocketAddr,
        dest: std::net::SocketAddr,
    ) -> i32 {
        self.inner_bingle_api_internal
            .turn_handle_call(source_id, dest_id, source, dest)
    }

    fn turn_handle_listen(&self, id: String, source: std::net::SocketAddr) -> bool {
        self.inner_bingle_api_internal
            .turn_handle_listen(id, source)
    }

    fn turn_handle_called(
        &self,
        source: std::net::SocketAddr,
        dest: std::net::SocketAddr,
        channel: u16,
    ) {
        self.inner_bingle_api_internal
            .turn_handle_called(source, dest, channel);
    }

    fn notify_listening(&self, listening: bool, nat_type: bingle_core::engine::NatType) {
        self.inner_bingle_api_internal
            .notify_listening(listening, nat_type);
    }

    fn set_relay_state(&self, state: bingle_core::engine::RelayState) {
        self.inner_bingle_api_internal.set_relay_state(state);
    }

    fn get_peer_ddb_target(&self) -> Option<usize> {
        self.inner_bingle_api_internal.get_peer_ddb_target()
    }

    fn ddb_upsert_record(&self, record: bingle_core::ddb::AdvertRecord) {
        self.inner_bingle_api_internal.ddb_upsert_record(record);
    }

    fn ddb_delete_record(&self, id: &str) {
        self.inner_bingle_api_internal.ddb_delete_record(id);
    }

    fn relay_finder_remove_relay(&self, relay_id: &str) {
        self.inner_bingle_api_internal
            .relay_finder_remove_relay(relay_id);
    }

    fn relay_finder_clear_state_cache(&self) {
        self.inner_bingle_api_internal
            .relay_finder_clear_state_cache();
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

    fn ripple_message(
        &self,
        message: serde_json::Value,
        originator_id: String,
        ddb_backend: &dyn bingle_core::ddb::DdbBackend,
    ) {
        self.inner_bingle_api_internal
            .ripple_message(message, originator_id, ddb_backend);
    }

    fn get_signing_key(&self) -> Option<ed25519_dalek::SigningKey> {
        self.inner_bingle_api_internal
            .get_signing_key()
            .or_else(|| self.inner_bingle_api.get_signing_key())
    }
}

/// Delegating trait: mirrors `bingle_core::api::bingle_api::BingleApiInternal` but provides defaults,
/// so test mocks can override only the methods they care about.
pub trait InnerBingleApiInternal {
    fn get_relay_state(&self) -> String {
        "off".to_string()
    }

    fn set_state(&self, _state: bingle_core::engine::EngineState) {}

    fn get_state(&self) -> bingle_core::engine::EngineState {
        bingle_core::engine::EngineState::StunIdentify
    }

    fn set_nat_type(&self, _nat: bingle_core::engine::NatType) {}

    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> {
        None
    }

    fn ddb_register_ip(
        &self,
        _endpoint: std::net::SocketAddr,
        _am_relay: bool,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".into(),
        ))
    }

    fn ddb_register_relay(
        &self,
        _relay_id: String,
        _relay_sig: Option<String>,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".into(),
        ))
    }

    fn update_turn_listener_relay(
        &self,
        _relay_id: String,
        _relay_addr: std::net::SocketAddr,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".into(),
        ))
    }

    fn turn_client_handle_listen_response(
        &self,
        _relay_addr: std::net::SocketAddr,
        _relay_id: String,
    ) {
    }

    fn turn_lookup_addr_by_id(&self, _id: String) -> Option<std::net::SocketAddr> {
        None
    }

    fn turn_handle_call(
        &self,
        _source_id: String,
        _dest_id: String,
        _source: std::net::SocketAddr,
        _dest: std::net::SocketAddr,
    ) -> i32 {
        -1
    }

    fn turn_handle_listen(&self, _id: String, _source: std::net::SocketAddr) -> bool {
        false
    }

    fn turn_handle_called(
        &self,
        _source: std::net::SocketAddr,
        _dest: std::net::SocketAddr,
        _channel: u16,
    ) {
    }

    fn notify_listening(&self, _listening: bool, _nat_type: bingle_core::engine::NatType) {}

    fn set_relay_state(&self, _state: bingle_core::engine::RelayState) {}

    fn get_peer_ddb_target(&self) -> Option<usize> {
        None
    }

    fn ddb_upsert_record(&self, _record: bingle_core::ddb::AdvertRecord) {}

    fn ddb_delete_record(&self, _id: &str) {}

    fn relay_finder_remove_relay(&self, _relay_id: &str) {}

    fn relay_finder_clear_state_cache(&self) {}

    fn ddb_backend_size(&self) -> usize {
        0
    }

    fn initialize_relay(&self) {}

    fn is_relay(&self) -> bool {
        false
    }

    fn signal_signon_complete(&self) {}

    fn reset_signon_complete(&self) {}

    fn ripple_message(
        &self,
        _message: serde_json::Value,
        _originator_id: String,
        _ddb_backend: &dyn bingle_core::ddb::DdbBackend,
    ) {
    }

    fn get_signing_key(&self) -> Option<ed25519_dalek::SigningKey> {
        None
    }
}

/// Test helper: wrap a concrete `BingleApiBoth` into a leaked `Arc<Mutex<dyn BingleApiBoth>>` and return a `Weak`.
/// This mirrors the helper used elsewhere in tests, but is scoped under `crate::util::reusable_mock_api`.
pub fn to_weak_api_both<T: BingleApiBoth + 'static>(
    api: T,
) -> bingle_core::api::bingle_api::BingleApiBothType {
    let arc: Arc<dyn BingleApiBoth> = Arc::new(api);
    let weak = Arc::downgrade(&arc);

    // Leak the Arc to keep it alive for the duration of the test process.
    Box::leak(Box::new(arc));

    weak
}

pub fn to_weak<T: BingleApiBoth + 'static>(
    api: T,
) -> bingle_core::api::bingle_api::BingleApiBothType {
    to_weak_api_both(api)
}
