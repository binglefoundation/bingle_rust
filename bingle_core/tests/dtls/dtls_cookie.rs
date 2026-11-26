// HelloVerifyRequest cookies (bingle_core `dtls::dtls_cookie`): binding to the peer and to a time
// bucket, and finding the cookie in a ClientHello.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bingle_core::dtls::dtls_cookie::{COOKIE_BUCKET, COOKIE_LEN, CookieKey, client_hello_cookie};

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

#[test]
pub fn a_cookie_verifies_for_its_peer_in_its_bucket_and_the_next() {
    let key = CookieKey::with_secret([7; 32]);
    let t = at(1_000_000 * COOKIE_BUCKET.as_secs());
    let cookie = key.make("127.0.0.1:4000", t).expect("make");
    assert_eq!(cookie.len(), COOKIE_LEN);
    assert!(key.verify("127.0.0.1:4000", &cookie, t));
    assert!(key.verify("127.0.0.1:4000", &cookie, t + COOKIE_BUCKET));
}

#[test]
pub fn a_cookie_fails_for_another_peer_another_key_or_once_expired() {
    let key = CookieKey::with_secret([7; 32]);
    let t = at(1_000_000 * COOKIE_BUCKET.as_secs());
    let cookie = key.make("127.0.0.1:4000", t).expect("make");
    assert!(!key.verify("127.0.0.1:4001", &cookie, t), "another port");
    assert!(
        !CookieKey::with_secret([8; 32]).verify("127.0.0.1:4000", &cookie, t),
        "another node's secret"
    );
    assert!(
        !key.verify("127.0.0.1:4000", &cookie, t + COOKIE_BUCKET * 2),
        "two buckets later"
    );
    assert!(!key.verify("127.0.0.1:4000", &cookie[..8], t), "truncated");
    assert!(
        !key.verify("127.0.0.1:4000", &[0u8; COOKIE_LEN], t),
        "zeros"
    );
}

/// A ClientHello datagram with the given session id and cookie, as fragment `offset`.
fn hello(session_id: &[u8], cookie: &[u8], offset: u32) -> Vec<u8> {
    let mut body = vec![0xfe, 0xfd];
    body.extend_from_slice(&[0x11; 32]);
    body.push(session_id.len() as u8);
    body.extend_from_slice(session_id);
    body.push(cookie.len() as u8);
    body.extend_from_slice(cookie);
    body.extend_from_slice(&[0x00, 0x02, 0xc0, 0x2b, 0x01, 0x00]);
    let len = body.len() as u32;
    let mut data = vec![0x16, 0xfe, 0xfd, 0, 0, 0, 0, 0, 0, 0, 1];
    data.extend_from_slice(&((len + 12) as u16).to_be_bytes());
    data.push(0x01);
    data.extend_from_slice(&len.to_be_bytes()[1..]);
    data.extend_from_slice(&[0, 1]);
    data.extend_from_slice(&offset.to_be_bytes()[1..]);
    data.extend_from_slice(&len.to_be_bytes()[1..]);
    data.extend_from_slice(&body);
    data
}

#[test]
pub fn the_cookie_is_found_in_a_client_hello() {
    assert_eq!(
        client_hello_cookie(&hello(&[], &[9; 16], 0)),
        Some(&[9u8; 16][..])
    );
    assert_eq!(
        client_hello_cookie(&hello(&[3; 32], &[9; 16], 0)),
        Some(&[9u8; 16][..]),
        "after a session id"
    );
    assert_eq!(client_hello_cookie(&hello(&[], &[], 0)), None, "no cookie");
    assert_eq!(
        client_hello_cookie(&hello(&[], &[9; 16], 100)),
        None,
        "a later fragment"
    );
    let mut short = hello(&[], &[9; 16], 0);
    short.truncate(70);
    assert_eq!(client_hello_cookie(&short), None, "truncated");
    let mut not_hello = hello(&[], &[9; 16], 0);
    not_hello[13] = 0x02;
    assert_eq!(client_hello_cookie(&not_hello), None, "not a ClientHello");
}
