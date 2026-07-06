// Common test implementations shared between Rust integration tests and iOS FFI test runners.
// The tests are written generically over a backend trait, so we can plug a lightweight
// iOS-safe backend (no network deps) and the real backend (AlgoOps) on non-iOS.

pub mod backends;

/// Trait implemented by backends used in test implementations.
/// It abstracts the operations needed by the test suites.
pub trait TestAlgo: Sized {
    /// Construct a new instance. If `use_dummy_config` is true, the backend should be
    /// configured with an unreachable network configuration to simulate network behavior
    /// without real dependencies.
    fn new(passphrase: Option<String>, address: Option<String>, use_dummy_config: bool) -> Self;

    fn create_address(&mut self, save: bool, always_new_address: bool) -> Result<String, String>;
    fn public_key_bytes(&self) -> Result<[u8; 32], String>;
    fn private_key_bytes(&self) -> Result<Vec<u8>, String>;
    fn contract_address(&self, app_id: u64) -> Result<String, String>;
    fn sign(&self, text: &str) -> Result<String, String>;
    fn verify(&self, text: &str, sig_b64: &str) -> Result<bool, String>;

    fn global_state(
        &self,
        maybe_app_id: Option<u64>,
    ) -> Result<Option<Vec<(u64, Vec<(String, String)>)>>, String>;

    fn local_state_for_account(
        &self,
        app_id: u64,
        account_address: &str,
    ) -> Result<Option<Vec<(String, String)>>, String>;

    fn send_algo(&self, to_address: &str, amount_algos: f64) -> Result<(), String>;
    fn create_asset(&self, name: &str, units_in_issue: u64) -> Result<Option<u64>, String>;
    fn opt_in_to_asset(&self, asset_id: u64) -> Result<(), String>;

    // Address helpers
    fn addr_from_pk(pk: &[u8; 32]) -> Result<String, String>;
    fn pk_from_addr(addr: &str) -> Result<[u8; 32], String>;
}

// ===== STUN Tests (generic) =====
pub fn stun_tests() -> bool {
    use rust_comms::stun::{StunEndpointFinder, StunEndpointFinderImpl, StunState};
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering as AOrdering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    fn make_xor_mapped_response(ip: [u8; 4], port: u16) -> Vec<u8> {
        // Build a minimal STUN success with XOR-MAPPED-ADDRESS for IPv4
        let mut pkt = vec![0u8; 20];
        // Message Type: 0x0101 (Binding Success Response)
        pkt[0] = 0x01;
        pkt[1] = 0x01;
        // We'll add one attribute of length 8
        pkt[2] = 0x00;
        pkt[3] = 0x0c; // 12 bytes (type+len + value)
        // Magic Cookie
        pkt[4] = 0x21;
        pkt[5] = 0x12;
        pkt[6] = 0xA4;
        pkt[7] = 0x42;
        // Transaction ID (12 bytes arbitrary)
        for i in 0..12 {
            pkt[8 + i] = i as u8;
        }
        // Attribute: XOR-MAPPED-ADDRESS (0x0020), length 8
        pkt.extend_from_slice(&[0x00, 0x20, 0x00, 0x08]);
        // Value: 0x00 family(0x01), x-port, x-address
        pkt.push(0x00);
        pkt.push(0x01);
        let xport = port ^ 0x2112;
        pkt.extend_from_slice(&xport.to_be_bytes());
        let mut xaddr = ip;
        let cookie = [0x21u8, 0x12, 0xA4, 0x42];
        for i in 0..4 {
            xaddr[i] ^= cookie[i];
        }
        pkt.extend_from_slice(&xaddr);
        pkt
    }

    // --- state_transitions_consistent_and_inconsistent ---
    {
        let mut finder = StunEndpointFinderImpl::new();
        let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
        let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();
        finder.start(vec![s1, s2], 50, 50);

        let changes = Arc::new(Mutex::new(Vec::<(StunState, Option<SocketAddr>)>::new()));
        let changes_clone = changes.clone();
        finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
            changes_clone.lock().unwrap().push((st, ep));
        })));

        // First response: SINGLE
        let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
        finder.process_packet(s1, &r1);

        // Second response from another server, same endpoint -> CONSISTENT
        let r2 = make_xor_mapped_response([203, 0, 113, 9], 55000);
        finder.process_packet(s2, &r2);

        // Now different endpoint from s2 -> INCONSISTENT
        let r3 = make_xor_mapped_response([203, 0, 113, 10], 55001);
        finder.process_packet(s2, &r3);

        // Verify callback recorded transitions
        let list = changes.lock().unwrap();
        if !list.iter().any(|(st, _)| *st == StunState::Single) {
            return false;
        }
        if !list.iter().any(|(st, _)| *st == StunState::Consistent) {
            return false;
        }
        if !list.iter().any(|(st, _)| *st == StunState::Inconsistent) {
            return false;
        }

        finder.stop();
    }

    // --- error_after_three_intervals_with_less_than_two_responders ---
    {
        let mut finder = StunEndpointFinderImpl::new();
        let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
        let _s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();
        finder.start(vec![s1, _s2], 5, 5);
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_clone = hits.clone();
        finder.set_error_handler(Some(Arc::new(move |msg| {
            if msg.contains("Fewer than 2 STUN servers responded") {
                hits_clone.fetch_add(1, AOrdering::SeqCst);
            }
        })));
        // Simulate only one server ever responding
        let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
        finder.process_packet(s1, &r1);
        // Wait until error handler invoked or timeout
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline && hits.load(AOrdering::SeqCst) < 1 {
            std::thread::sleep(Duration::from_millis(5));
        }
        if hits.load(AOrdering::SeqCst) < 1 {
            return false;
        }
        finder.stop();
    }

    // --- stop_stops_promptly ---
    {
        let mut finder = StunEndpointFinderImpl::new();
        finder.start(vec![], 5000, 5000);
        std::thread::sleep(Duration::from_millis(100));
        let start = Instant::now();
        finder.stop();
        if start.elapsed() >= Duration::from_millis(500) {
            return false;
        }
    }

    true
}

// ===== Test suites (generic) =====

pub fn algo_ops_basic<T: TestAlgo>(passphrase: &str) -> bool {
    // Roundtrip pk <-> address
    let mut pk = [0u8; 32];
    for i in 0..32 {
        pk[i] = i as u8;
    }
    let addr = match T::addr_from_pk(&pk) {
        Ok(a) => a,
        Err(_) => return false,
    };
    let decoded = match T::pk_from_addr(&addr) {
        Ok(d) => d,
        Err(_) => return false,
    };
    if decoded != pk {
        return false;
    }

    // public_key_bytes requires address (use non-derivable passphrase so Real backend doesn't auto-set address)
    let ops = T::new(Some("invalid-passphrase".to_string()), None, false);
    let err = ops.public_key_bytes().err().unwrap_or_default();
    if !err.contains("needs an address") {
        return false;
    }

    // invalid address rejected
    if T::pk_from_addr("INVALIDADDRESS123").is_ok() {
        return false;
    }

    // public_key_bytes with valid address
    let addr2 = match T::addr_from_pk(&pk) {
        Ok(a) => a,
        Err(_) => return false,
    };
    let ops2 = T::new(Some(passphrase.to_string()), Some(addr2), false);
    let extracted = match ops2.public_key_bytes() {
        Ok(p) => p,
        Err(_) => return false,
    };
    if extracted != pk {
        return false;
    }
    true
}

pub fn algo_ops_more<T: TestAlgo>(passphrase: &str) -> bool {
    let mut ops = T::new(Some(passphrase.to_string()), None, false);
    let addr = match ops.create_address(false, false) {
        Ok(a) => a,
        Err(_) => return false,
    };
    if addr.is_empty() {
        return false;
    }
    if ops.public_key_bytes().map(|pk| pk.len()).unwrap_or(0) != 32 {
        return false;
    }
    if ops.private_key_bytes().map(|sk| sk.len()).unwrap_or(0) != 32 {
        return false;
    }
    let pk = match ops.public_key_bytes() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let addr2 = match T::addr_from_pk(&pk) {
        Ok(a) => a,
        Err(_) => return false,
    };
    if addr2 != addr {
        return false;
    }

    // sign/verify/tamper
    let text = "Hello from Rust";
    let sig = match ops.sign(text) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let verifier = T::new(Some(passphrase.to_string()), Some(addr.clone()), false);
    if !verifier.verify(text, &sig).unwrap_or(false) {
        return false;
    }
    let mut s = sig.into_bytes();
    if !s.is_empty() {
        s[0] ^= 0x01;
    }
    let tampered = String::from_utf8(s).unwrap_or_default();
    let ok2 = T::new(Some(passphrase.to_string()), Some(addr), false)
        .verify(text, &tampered)
        .unwrap_or(true);
    if ok2 {
        return false;
    }

    // contract address spec
    let ops3 = T::new(Some(passphrase.to_string()), None, false);
    let app_id = 42u64;
    let addr = match ops3.contract_address(app_id) {
        Ok(a) => a,
        Err(_) => return false,
    };
    let pk2 = match T::pk_from_addr(&addr) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let mut hasher = sha2::Sha512_256::new();
    use sha2::Digest;
    hasher.update(b"appID");
    hasher.update(app_id.to_be_bytes());
    let expected: [u8; 32] = hasher.finalize().into();
    if pk2 != expected {
        return false;
    }

    // global state: missing address -> error (use non-derivable passphrase to avoid auto address)
    let ops4 = T::new(Some("invalid-passphrase".to_string()), None, false);
    if ops4.global_state(None).is_ok() {
        return false;
    }

    // local/global: valid address + unreachable -> None (with dummy config)
    let mut pk3 = [0u8; 32];
    for i in 0..32 {
        pk3[i] = i as u8;
    }
    let addr3 = match T::addr_from_pk(&pk3) {
        Ok(a) => a,
        Err(_) => return false,
    };
    let ops5 = T::new(Some(passphrase.to_string()), Some(addr3.clone()), true);
    if ops5.global_state(None).ok().flatten().is_some() {
        return false;
    }
    let ops6 = T::new(Some(passphrase.to_string()), None, true);
    if ops6
        .local_state_for_account(1, &addr3)
        .ok()
        .flatten()
        .is_some()
    {
        return false;
    }

    // send_algo validations
    // Construct without passphrase but with a valid address to trigger "account access" error
    let ops7 = T::new(None, Some(addr3.clone()), false);
    if !ops7
        .send_algo("SOMEADDR", 1.0)
        .err()
        .unwrap_or_default()
        .contains("account access")
    {
        return false;
    }

    let mut ops8 = T::new(Some(passphrase.to_string()), None, false);
    let a = match ops8.create_address(false, false) {
        Ok(x) => x,
        Err(_) => return false,
    };
    if !ops8
        .send_algo(&a, 0.0)
        .err()
        .unwrap_or_default()
        .contains("amount must be positive")
    {
        return false;
    }
    if !ops8
        .send_algo(&a, -1.0)
        .err()
        .unwrap_or_default()
        .contains("amount must be positive")
    {
        return false;
    }

    let mut ops9 = T::new(Some(passphrase.to_string()), None, true);
    let a2 = match ops9.create_address(false, false) {
        Ok(x) => x,
        Err(_) => return false,
    };
    if ops9.send_algo("NOTANADDRESS", 0.001).is_ok() {
        return false;
    }
    if ops9.send_algo(&a2, 0.001).is_ok() {
        return false;
    }

    true
}

pub fn asset_ops<T: TestAlgo>(passphrase: &str) -> bool {
    let mut ops = T::new(Some(passphrase.to_string()), None, false);
    let _ = match ops.create_address(false, false) {
        Ok(a) => a,
        Err(_) => return false,
    };

    if !ops
        .create_asset("", 1000)
        .err()
        .unwrap_or_default()
        .contains("asset name")
    {
        return false;
    }
    if !ops
        .create_asset("TKN", 0)
        .err()
        .unwrap_or_default()
        .contains("units_in_issue")
    {
        return false;
    }

    // Provide address but no key => account access error
    let mut pk = [0u8; 32];
    for i in 0..32 {
        pk[i] = i as u8;
    }
    let addr = match T::addr_from_pk(&pk) {
        Ok(a) => a,
        Err(_) => return false,
    };
    let ops2 = T::new(None, Some(addr), false);
    if !ops2
        .create_asset("TKN", 1000)
        .err()
        .unwrap_or_default()
        .contains("account access")
    {
        return false;
    }

    let mut pk2 = [1u8; 32];
    for i in 0..32 {
        pk2[i] = (255 - i as u8);
    }
    let addr2 = match T::addr_from_pk(&pk2) {
        Ok(a) => a,
        Err(_) => return false,
    };
    let ops3 = T::new(None, Some(addr2), false);
    if !ops3
        .opt_in_to_asset(1234)
        .err()
        .unwrap_or_default()
        .contains("account access")
    {
        return false;
    }

    true
}
