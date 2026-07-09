// Integration test for the "restart sticks at TrianglePing" bug.
//
// The Engine object is reused across a stop/start cycle (the app can restart the node
// without recreating the Engine). Before the fix, Engine::stop left stale runtime state
// behind — in particular self.state == TrianglePing from the prior registration. On restart
// the first STUN-consistent callback ran stun_consistent_process, which hit the
// `if self.state == TrianglePing { return }` guard and early-returned ("already in
// TrianglePing"), so TriangleTest1 was never re-sent and the node never progressed to
// EndpointAvailable/NATRestricted and thence Registered.
//
// Observed in the field (abridged) after a restart:
//   [Engine] on_stun_consistent: public_addr=Some(206.83.102.41:3132)
//   [Engine] public address changed: Some(...:21176) -> Some(...:3132); resetting peers
//   [ENGINE][Engine] set_last_public_addr: Some(206.83.102.41:3132)
//   [Engine] already in TrianglePing              <-- stuck here forever
//
// The fix resets the engine's runtime identification state in Engine::stop so a restart
// re-runs STUN -> triangle -> register from a clean slate.

use std::net::SocketAddr;
use std::sync::Arc;

use bingle_core::api::bingle_api::{NetworkEndpoint, StartOptions};
use bingle_core::dtls::network_mux_udp::UdpNetworkMux;
use bingle_core::dtls::{Dtls, HandleMessage, HandlePeerCertificate};
use bingle_core::engine::{Engine, EngineState};
use bingle_core::messages::router::Router;

use crate::util::reusable_mock_api::{MockApiBoth, to_weak_api_both};

// ---------------------------------------------------------------------------
// Minimal DTLS stub (mirrors the other engine tests) so Engine::stop can tear
// down without a real network stack.
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct NullDtls;

impl Dtls for NullDtls {
    fn start(&self, _mux: Arc<UdpNetworkMux>) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn stop(&self) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> bingle_core::dtls::Result<()> {
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> {
        None
    }
    fn set_handle_message(&self, _handler: Option<HandleMessage>) {}
    fn set_handle_new_session(
        &self,
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
    fn set_handle_peer_certificate(&self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_ca_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_ca_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_client_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_client_private_key(&self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_cert(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_cert(&self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> {
        None
    }
    fn set_server_signing_private_key(&self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_app_layer_only_verification(&self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_dangerous_debug(&self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn set_null_encryption(&self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn get_cipher_suite(&self, _endpoint: &NetworkEndpoint) -> Option<String> {
        None
    }
    fn forget_peers(&self) {}
}

fn build_engine() -> Engine {
    let api = to_weak_api_both(MockApiBoth::new());
    let mut eng = Engine::new_with_dtls(
        &StartOptions::new("restart_client".into()),
        api.clone(),
        Box::new(NullDtls),
    );
    eng.set_router(Arc::new(Router::new(api)));
    eng
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn engine_restart_reidentifies_after_stop() {
    let mut eng = build_engine();

    let addr_p: SocketAddr = "206.83.102.41:21176".parse().unwrap();
    let relay_addr: SocketAddr = "34.224.38.163:12121".parse().unwrap();
    let relay = crate::util::test_util::signed_root_relay("RELAYONE", relay_addr);

    // ---- Session 1: reach the settled post-identification state ----
    // Simulate STUN-consistent on P discovering a relay: the engine enters TrianglePing and
    // remembers P. In production self.state is left at TrianglePing after the triangle /
    // registration completes and is never reset.
    eng.test_stun_consistent_process_with_relays(addr_p, vec![relay.clone()]);
    assert_eq!(
        eng.state(),
        EngineState::TrianglePing,
        "session 1 should reach TrianglePing"
    );
    assert_eq!(
        eng.last_public_addr(),
        Some(addr_p),
        "session 1 should remember public addr P"
    );

    // ---- Stop (node stopped; the Engine object is reused, not recreated) ----
    eng.stop();

    // ---- Restart readiness: stop() must reset runtime identification state ----
    // Before the fix these both failed: self.state stayed TrianglePing and last_public_addr
    // stayed Some(P), so the next stun_consistent_process on restart early-returned at
    // "already in TrianglePing" and TriangleTest1 was never re-sent.
    assert_eq!(
        eng.state(),
        EngineState::StunIdentify,
        "after stop the engine must reset to StunIdentify so a restart re-runs identification"
    );
    assert_eq!(
        eng.last_public_addr(),
        None,
        "after stop the engine must forget the previous public address so a restart is a fresh identification"
    );

    // ---- Session 2 (restart): a new STUN-consistent must progress again ----
    // On real hardware the socket rebinds to a new source port, so the public mapping differs
    // (here P -> Q). Because stop() cleared last_public_addr, this is treated as a fresh
    // identification (not an address change) and the engine re-enters TrianglePing rather
    // than sticking at "already in TrianglePing".
    let addr_q: SocketAddr = "206.83.102.41:3132".parse().unwrap();
    eng.test_stun_consistent_process_with_relays(addr_q, vec![relay]);
    assert_eq!(
        eng.state(),
        EngineState::TrianglePing,
        "after restart the engine must re-identify (TrianglePing), not stick"
    );
    assert_eq!(
        eng.last_public_addr(),
        Some(addr_q),
        "restart should adopt the new public addr Q"
    );

    eng.stop();
}
