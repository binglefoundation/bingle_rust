// Grouped DTLS tests

#[path = "dtls_client_echo_roundtrip.rs"]
pub mod dtls_client_echo_roundtrip;

#[path = "dtls_external_openssl_server.rs"]
pub mod dtls_external_openssl_server;

#[path = "dtls_loopback_e2e.rs"]
pub mod dtls_loopback_e2e;

#[path = "dtls_multi_client_loopback_e2e.rs"]
pub mod dtls_multi_client_loopback_e2e;

#[path = "dtls_openssl_smoke.rs"]
pub mod dtls_openssl_smoke;

#[path = "dtls_peer_certificate_handlers.rs"]
pub mod dtls_peer_certificate_handlers;

#[path = "dtls_peer_certificate_rejection.rs"]
pub mod dtls_peer_certificate_rejection;

#[path = "dtls_start_with_network_mux.rs"]
pub mod dtls_start_with_network_mux;

#[path = "dtls_client_keeps_stream_open.rs"]
pub mod dtls_client_keeps_stream_open;

#[path = "dtls_stun_interleave_handshake.rs"]
pub mod dtls_stun_interleave_handshake;

#[path = "network_mux_udp_tests.rs"]
pub mod network_mux_udp_tests;

#[path = "network_mux_udp_reprocess.rs"]
pub mod network_mux_udp_reprocess;

#[path = "dtls_debug_alert.rs"]
pub mod dtls_debug_alert;

#[path = "dtls_debug_sequence.rs"]
pub mod dtls_debug_sequence;

#[path = "pki.rs"]
pub mod pki;
#[path = "test_handlers.rs"]
pub mod test_handlers;
#[path = "dtls_client_reconnect.rs"]
pub mod dtls_client_reconnect;


#[path = "dtls_debug_handshake.rs"]
pub mod dtls_debug_handshake;

#[path = "dtls_issuer_mapping.rs"]
pub mod dtls_issuer_mapping;

#[path = "dtls_peer_certificate_issuer_trim.rs"]
pub mod dtls_peer_certificate_issuer_trim;

#[path = "dtls_send_signature.rs"]
pub mod dtls_send_signature;
#[path = "network_mux_udp_socket_close.rs"]
pub mod network_mux_udp_socket_close;
#[path = "dtls_handshake_failure_retry.rs"]
pub mod dtls_handshake_failure_retry;

#[path = "dtls_send_stream_lock_contention.rs"]
pub mod dtls_send_stream_lock_contention;

#[path = "dtls_peer_worker_stage1.rs"]
pub mod dtls_peer_worker_stage1;

#[path = "peer_cmd_display.rs"]
pub mod peer_cmd_display;