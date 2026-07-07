use openssl::ssl::SslOptions;

#[path = "../test_util.rs"]
pub mod test_util;

// Note: the live-handshake companion test (dtls_handshake_succeeds_with_renegotiation_disabled)
// was flaky and has been moved to tests/flaky/renegotiation_handshake.rs. This config-only
// check is deterministic and stays in the unit suite.
#[test]
fn dtls_configuration_disables_renegotiation() {
    test_util::init_test_logging();

    use openssl::ssl::{SslAcceptor, SslConnector, SslMethod};

    // Verify client configuration
    let mut connector_builder =
        SslConnector::builder(SslMethod::dtls()).expect("connector builder");
    bingle_core::dtls::dtls_openssl::openssl_impl::configure_dtls12_connector(
        &mut connector_builder,
        "test".to_string(),
        false,
    )
    .expect("configure connector");
    let connector_options = connector_builder.options();
    assert!(
        connector_options.contains(SslOptions::NO_RENEGOTIATION),
        "Client: NO_RENEGOTIATION should be set"
    );

    // Verify server configuration
    let mut acceptor_builder =
        SslAcceptor::mozilla_intermediate_v5(SslMethod::dtls()).expect("acceptor builder");
    bingle_core::dtls::dtls_openssl::openssl_impl::configure_dtls12_acceptor(
        &mut acceptor_builder,
        "test".to_string(),
        false,
    )
    .expect("configure acceptor");
    let acceptor_options = acceptor_builder.options();
    assert!(
        acceptor_options.contains(SslOptions::NO_RENEGOTIATION),
        "Server: NO_RENEGOTIATION should be set"
    );

    println!(
        "Verified that both connector and acceptor configurations explicitly disable renegotiation."
    );
}
