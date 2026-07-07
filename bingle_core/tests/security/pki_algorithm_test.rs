use openssl::pkey::Id;
use openssl::x509::X509;
use bingle_core::api::pki::generate_pki_from_ops;

#[path = "../test_util.rs"]
pub mod test_util;

#[test]
fn pki_generation_uses_expected_algorithms() {
    test_util::init_test_logging();

    let ops = test_util::ops_from_mnemonic(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        test_util::localnet_config(),
    );

    let (ca_pem, srv_crt_pem, _srv_key_pem, cli_crt_pem, _cli_key_pem) =
        generate_pki_from_ops(&ops).expect("pki generation failed");

    // 1) Verify CA certificate is Ed25519
    let ca_cert = X509::from_pem(&ca_pem).expect("failed to parse CA cert");
    let ca_pubkey = ca_cert.public_key().expect("failed to get CA pubkey");
    assert_eq!(
        ca_pubkey.id(),
        Id::ED25519,
        "CA public key should be Ed25519"
    );

    // 2) Verify Server certificate is EC (ECDHE-ready)
    let srv_cert = X509::from_pem(&srv_crt_pem).expect("failed to parse server cert");
    let srv_pubkey = srv_cert.public_key().expect("failed to get server pubkey");
    assert_eq!(srv_pubkey.id(), Id::EC, "Server public key should be EC");

    // 3) Verify Client certificate is EC (ECDHE-ready)
    let cli_cert = X509::from_pem(&cli_crt_pem).expect("failed to parse client cert");
    let cli_pubkey = cli_cert.public_key().expect("failed to get client pubkey");
    assert_eq!(cli_pubkey.id(), Id::EC, "Client public key should be EC");

    println!("Verified: CA is Ed25519, Peer certificates are EC");
}
