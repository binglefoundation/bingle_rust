// Grouped DTLS tests

#[path = "dtls_client_echo_roundtrip.rs"]
mod dtls_client_echo_roundtrip;

#[path = "dtls_external_openssl_server.rs"]
mod dtls_external_openssl_server;

#[path = "dtls_loopback_e2e.rs"]
mod dtls_loopback_e2e;

#[path = "dtls_multi_client_loopback_e2e.rs"]
mod dtls_multi_client_loopback_e2e;

#[path = "dtls_openssl_smoke.rs"]
mod dtls_openssl_smoke;

#[path = "dtls_peer_certificate_handlers.rs"]
mod dtls_peer_certificate_handlers;

#[path = "dtls_peer_certificate_rejection.rs"]
mod dtls_peer_certificate_rejection;

#[path = "dtls_start_with_network_mux.rs"]
mod dtls_start_with_network_mux;

#[path = "dtls_client_keeps_stream_open.rs"]
mod dtls_client_keeps_stream_open;

#[path = "dtls_stun_interleave_handshake.rs"]
mod dtls_stun_interleave_handshake;

#[path = "network_mux_udp_tests.rs"]
mod network_mux_udp_tests;

#[path = "network_mux_udp_reprocess.rs"]
mod network_mux_udp_reprocess;

#[path = "dtls_debug_alert.rs"]
mod dtls_debug_alert;

#[path = "dtls_debug_sequence.rs"]
mod dtls_debug_sequence;

#[path = "pki.rs"]
mod pki;
