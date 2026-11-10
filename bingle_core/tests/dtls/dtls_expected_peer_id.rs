// A send fails when the peer at the endpoint verifies as an id other than the one expected there,
// whether the session is new, reused, or was accepted from the peer. All certificates here are
// genuine: the peer is who it says it is, just not who the sender meant.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::dtls_forged_ca_rejection::{genuine, start_client, start_server};
use crate::util::test_util::{
    ADDRESS_10MIL, ADDRESS_RECEIVE, ADDRESS_SPEND, PASSPHRASE_10MIL, PASSPHRASE_SPEND,
    init_test_logging,
};
use bingle_core::api::bingle_api::NetworkEndpoint;
use bingle_core::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};
use bingle_core::protocol::cert_verify::peer_certificate_handler;

/// Wait up to `wait` for `seen` to hold `count` entries; return how many it holds.
fn wait_for_count<T>(seen: &Arc<Mutex<Vec<T>>>, count: usize, wait: Duration) -> usize {
    let start = Instant::now();
    while start.elapsed() < wait {
        if seen.lock().unwrap().len() >= count {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    seen.lock().unwrap().len()
}

/// Control: a new session to the expected id carries the message.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_send_to_expected_id_is_delivered() {
    init_test_logging();
    let (_server, _smux, addr, seen) = start_server(
        &genuine(PASSPHRASE_10MIL, ADDRESS_10MIL),
        peer_certificate_handler(),
    );
    let (client, _cmux) = start_client(
        &genuine(PASSPHRASE_SPEND, ADDRESS_SPEND),
        peer_certificate_handler(),
    );
    let to = NetworkEndpoint::new_direct(addr);
    client.expect_peer_id(&to, ADDRESS_10MIL);

    assert_eq!(client.send(&to, b"hello"), Ok(()));
    assert_eq!(wait_for_count(&seen, 1, Duration::from_secs(5)), 1);
}

/// A new session that verifies as another id fails the send before any data is written.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_new_session_to_other_id_fails_send() {
    init_test_logging();
    let (_server, _smux, addr, seen) = start_server(
        &genuine(PASSPHRASE_10MIL, ADDRESS_10MIL),
        peer_certificate_handler(),
    );
    let (client, _cmux) = start_client(
        &genuine(PASSPHRASE_SPEND, ADDRESS_SPEND),
        peer_certificate_handler(),
    );
    let to = NetworkEndpoint::new_direct(addr);
    client.expect_peer_id(&to, ADDRESS_RECEIVE);

    let res = client.send(&to, b"for-receive-only");
    assert!(res.is_err(), "send to the wrong id succeeded");
    assert_eq!(
        wait_for_count(&seen, 1, Duration::from_secs(2)),
        0,
        "the wrong id received the message"
    );
}

/// A reused session that verified as another id fails the send.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_reused_session_to_other_id_fails_send() {
    init_test_logging();
    let (_server, _smux, addr, seen) = start_server(
        &genuine(PASSPHRASE_10MIL, ADDRESS_10MIL),
        peer_certificate_handler(),
    );
    let (client, _cmux) = start_client(
        &genuine(PASSPHRASE_SPEND, ADDRESS_SPEND),
        peer_certificate_handler(),
    );
    let to = NetworkEndpoint::new_direct(addr);
    client.expect_peer_id(&to, ADDRESS_10MIL);
    assert_eq!(client.send(&to, b"first"), Ok(()));
    assert_eq!(wait_for_count(&seen, 1, Duration::from_secs(5)), 1);

    client.expect_peer_id(&to, ADDRESS_RECEIVE);
    let res = client.send(&to, b"for-receive-only");
    assert!(
        res.is_err(),
        "send over a session to the wrong id succeeded"
    );
    assert_eq!(
        wait_for_count(&seen, 2, Duration::from_secs(2)),
        1,
        "the wrong id received the message"
    );
}

/// A reply over a session the peer opened fails when the peer isn't the expected id.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_reply_over_inbound_session_to_other_id_fails_send() {
    init_test_logging();
    // Server records where each message came from, so it can reply.
    let server_creds = genuine(PASSPHRASE_10MIL, ADDRESS_10MIL);
    let smux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    let addr: SocketAddr = smux.local_addr().expect("mux addr");
    let froms: Arc<Mutex<Vec<NetworkEndpoint>>> = Arc::new(Mutex::new(Vec::new()));
    let froms_clone = froms.clone();
    let server = DtlsOpenSsl::new("server".to_string())
        .with_handle_peer_certificate(peer_certificate_handler())
        .with_server_signing_cert(server_creds.server_crt.clone())
        .with_server_signing_private_key(server_creds.server_key.clone())
        .with_ca_cert(server_creds.ca.clone())
        .with_handle_message(Arc::new(move |_server, from, _issuer, _data| {
            froms_clone.lock().unwrap().push(from.clone());
        }));
    smux.start().expect("mux start");
    server.start(smux.clone()).expect("server start");

    // Client records the replies it receives.
    let (client, _cmux) = start_client(
        &genuine(PASSPHRASE_SPEND, ADDRESS_SPEND),
        peer_certificate_handler(),
    );
    let replies: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let replies_clone = replies.clone();
    client.set_handle_message(Some(Arc::new(move |_c, _from, _issuer, data| {
        replies_clone.lock().unwrap().push(data.to_vec());
    })));

    let to = NetworkEndpoint::new_direct(addr);
    assert_eq!(client.send(&to, b"hello"), Ok(()));
    assert_eq!(wait_for_count(&froms, 1, Duration::from_secs(5)), 1);
    let from = froms.lock().unwrap()[0].clone();

    // Control: replying to the client's real id works.
    server.expect_peer_id(&from, ADDRESS_SPEND);
    assert_eq!(server.send(&from, b"reply-1"), Ok(()));
    assert_eq!(wait_for_count(&replies, 1, Duration::from_secs(5)), 1);

    server.expect_peer_id(&from, ADDRESS_RECEIVE);
    let res = server.send(&from, b"for-receive-only");
    assert!(res.is_err(), "reply to the wrong id succeeded");
    assert_eq!(
        wait_for_count(&replies, 2, Duration::from_secs(2)),
        1,
        "the wrong id received the reply"
    );
}
