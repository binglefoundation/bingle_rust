

use rust_comms::api::pki::generate_pki_from_ops;
use rust_comms::protocol::{ISSUER_SUFFIX, VIRTUAL_CA};
use rust_comms::protocol::cert_verify::{dump_ca_public_key_info, dump_cert_info};
use rust_comms::blockchain::algo_ops::AlgoOps;
use crate::util::test_util::init_test_logging;

// Import predefined test passphrases and addresses
#[path = "../test_util.rs"]
pub mod test_util;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn generate_pki_from_ops_produces_valid_chain_and_expected_cns() {
    init_test_logging();
    
    // Initialize AlgoOps using predefined mnemonic and address
    let passphrase = test_util::PASSPHRASE_SPEND.to_string();
    let address = test_util::ADDRESS_SPEND.to_string();
    let ops = AlgoOps::new(Some(passphrase), Some(address.clone()), None);

    let (ca_pem, server_cert_pem, _server_key_pem, client_cert_pem, _client_key_pem) =
        generate_pki_from_ops(&ops).expect("generate pki");

    // Parse X509s
    use openssl::x509::X509;
    use openssl::nid::Nid;
    use openssl::pkey::Id;
    let ca = X509::from_pem(&ca_pem).expect("parse ca pem");
    let sc = X509::from_pem(&server_cert_pem).expect("parse server cert pem");
    let cc = X509::from_pem(&client_cert_pem).expect("parse client cert pem");

    // Dump all generated certificates (emit PEM)
    dump_cert_info("ca_gen", &ca, true);
    dump_cert_info("server_gen", &sc, true);
    dump_cert_info("client_gen", &cc, true);

    // CA CN == VIRTUAL_CA
    let ca_cn = ca.subject_name().entries_by_nid(Nid::COMMONNAME).next().and_then(|e| e.data().as_utf8().ok()).map(|s| s.to_string()).expect("ca cn");
    assert_eq!(ca_cn, VIRTUAL_CA);
    // CA OrganizationName (O) equals the provided address
    let ca_org = ca.subject_name().entries_by_nid(Nid::ORGANIZATIONNAME).next().and_then(|e| e.data().as_utf8().ok()).map(|s| s.to_string()).expect("ca org");
    assert_eq!(ca_org, address);

    // Issuer for server/client equals VIRTUAL_CA
    let sc_issuer = sc.issuer_name().entries_by_nid(Nid::COMMONNAME).next().and_then(|e| e.data().as_utf8().ok()).map(|s| s.to_string()).expect("sc issuer cn");
    let cc_issuer = cc.issuer_name().entries_by_nid(Nid::COMMONNAME).next().and_then(|e| e.data().as_utf8().ok()).map(|s| s.to_string()).expect("cc issuer cn");
    assert_eq!(sc_issuer, VIRTUAL_CA);
    assert_eq!(cc_issuer, VIRTUAL_CA);

    // Subjects end with ISSUER_SUFFIX
    let sc_subj = sc.subject_name().entries_by_nid(Nid::COMMONNAME).next().and_then(|e| e.data().as_utf8().ok()).map(|s| s.to_string()).expect("sc subj cn");
    let cc_subj = cc.subject_name().entries_by_nid(Nid::COMMONNAME).next().and_then(|e| e.data().as_utf8().ok()).map(|s| s.to_string()).expect("cc subj cn");
    assert!(sc_subj.ends_with(ISSUER_SUFFIX));
    assert!(cc_subj.ends_with(ISSUER_SUFFIX));

    // Verify CA is self-signed and Ed25519 public key
    let ca_pub = ca.public_key().expect("ca pub");
    // Dump the extracted CA public key using the shared diagnostic helper
    dump_ca_public_key_info(&ca_pub);
    assert_eq!(ca_pub.id(), Id::ED25519);
    assert!(ca.verify(&ca_pub).unwrap_or(false), "ca self-verify");

    // Verify server/client cert signatures against CA public key
    assert!(sc.verify(&ca_pub).unwrap_or(false), "server verify with ca");
    assert!(cc.verify(&ca_pub).unwrap_or(false), "client verify with ca");

    // Run the peer_certificate_handler against the generated server cert and CA
    let handler = rust_comms::protocol::cert_verify::peer_certificate_handler();
    let res = handler(&server_cert_pem, &ca_pem);
    assert!(res.is_ok(), "peer_certificate_handler rejected generated chain: {:?}", res.err());
    assert_eq!(res.unwrap(), sc_subj.to_string());
}
