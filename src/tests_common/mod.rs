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

// ===== Test suites (generic) =====

pub fn algo_ops_basic<T: TestAlgo>() -> bool {
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

    // public_key_bytes requires address
    let ops = T::new(None, None, false);
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
    let ops2 = T::new(None, Some(addr2), false);
    let extracted = match ops2.public_key_bytes() {
        Ok(p) => p,
        Err(_) => return false,
    };
    if extracted != pk {
        return false;
    }
    true
}

pub fn algo_ops_more<T: TestAlgo>() -> bool {
    let mut ops = T::new(None, None, false);
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
    let verifier = T::new(None, Some(addr.clone()), false);
    if !verifier.verify(text, &sig).unwrap_or(false) {
        return false;
    }
    let mut s = sig.into_bytes();
    if !s.is_empty() {
        s[0] ^= 0x01;
    }
    let tampered = String::from_utf8(s).unwrap_or_default();
    let ok2 = T::new(None, Some(addr), false)
        .verify(text, &tampered)
        .unwrap_or(true);
    if ok2 {
        return false;
    }

    // contract address spec
    let ops3 = T::new(None, None, false);
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

    // global state: missing address -> error
    let ops4 = T::new(None, None, false);
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
    let ops5 = T::new(None, Some(addr3.clone()), true);
    if ops5.global_state(None).ok().flatten().is_some() {
        return false;
    }
    let ops6 = T::new(None, None, true);
    if ops6
        .local_state_for_account(1, &addr3)
        .ok()
        .flatten()
        .is_some()
    {
        return false;
    }

    // send_algo validations
    let ops7 = T::new(None, None, false);
    if !ops7
        .send_algo("SOMEADDR", 1.0)
        .err()
        .unwrap_or_default()
        .contains("account access")
    {
        return false;
    }

    let mut ops8 = T::new(None, None, false);
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

    let mut ops9 = T::new(None, None, true);
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

pub fn asset_ops<T: TestAlgo>() -> bool {
    let mut ops = T::new(None, None, false);
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
        pk2[i] = (255 - i as u8) as u8;
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
