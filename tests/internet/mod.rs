// Tests that require live internet access (external STUN/testnet servers).
// Referenced from internet_all.rs.

// Live STUN test over real internet UDP
#[path = "../stun/stun_live_udp_mux.rs"]
pub mod stun_live_udp_mux;
