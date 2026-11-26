//! DTLS HelloVerifyRequest cookies (RFC 6347 §4.2.1).
//!
//! A cookie is an HMAC-SHA256 over the peer's endpoint key and a time bucket, under a random key
//! held by one DTLS instance. A client can only return a valid cookie if it received our
//! HelloVerifyRequest, so a valid cookie shows that the sender really is at that address. The
//! accept path relies on this before it replaces an established session (see `dtls_openssl`).

use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Length of a cookie in bytes: the HMAC-SHA256 output truncated to 16 bytes.
pub const COOKIE_LEN: usize = 16;

/// How long one time bucket lasts. A cookie verifies in its own bucket and the next, so it stays
/// valid for at least one bucket and at most two.
pub const COOKIE_BUCKET: Duration = Duration::from_secs(30);

/// Makes and checks cookies under one random secret.
pub struct CookieKey {
    secret: [u8; 32],
}

impl CookieKey {
    /// A key with a fresh random secret.
    pub fn new() -> Result<Self, String> {
        let mut secret = [0u8; 32];
        openssl::rand::rand_bytes(&mut secret).map_err(|e| format!("cookie secret: {}", e))?;
        Ok(Self { secret })
    }

    /// A key with the given secret (tests).
    pub fn with_secret(secret: [u8; 32]) -> Self {
        Self { secret }
    }

    /// The cookie for `peer` in the bucket containing `now`.
    pub fn make(&self, peer: &str, now: SystemTime) -> Result<[u8; COOKIE_LEN], String> {
        self.make_for_bucket(peer, bucket_of(now))
    }

    /// Whether `cookie` is the cookie for `peer` in the bucket containing `now` or the one before.
    pub fn verify(&self, peer: &str, cookie: &[u8], now: SystemTime) -> bool {
        if cookie.len() != COOKIE_LEN {
            return false;
        }
        let bucket = bucket_of(now);
        [bucket, bucket.saturating_sub(1)].iter().any(|b| {
            self.make_for_bucket(peer, *b)
                .map(|expected| openssl::memcmp::eq(&expected, cookie))
                .unwrap_or(false)
        })
    }

    fn make_for_bucket(&self, peer: &str, bucket: u64) -> Result<[u8; COOKIE_LEN], String> {
        let key = PKey::hmac(&self.secret).map_err(|e| format!("cookie hmac key: {}", e))?;
        let mut signer = Signer::new(MessageDigest::sha256(), &key)
            .map_err(|e| format!("cookie hmac: {}", e))?;
        signer
            .update(&bucket.to_be_bytes())
            .and_then(|_| signer.update(peer.as_bytes()))
            .map_err(|e| format!("cookie hmac update: {}", e))?;
        let mac = signer
            .sign_to_vec()
            .map_err(|e| format!("cookie hmac sign: {}", e))?;
        let mut cookie = [0u8; COOKIE_LEN];
        cookie.copy_from_slice(&mac[..COOKIE_LEN]);
        Ok(cookie)
    }
}

fn bucket_of(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / COOKIE_BUCKET.as_secs())
        .unwrap_or(0)
}

/// The cookie carried by a datagram whose first record is the first fragment of a DTLS
/// `ClientHello`, or `None` when there is none, it is empty, or the datagram isn't such a record.
///
/// Layout: a 13-byte record header, a 12-byte handshake header (type, length, message_seq,
/// fragment_offset, fragment_length), then client_version (2), random (32), session_id (1-byte
/// length plus bytes) and cookie (1-byte length plus bytes).
pub fn client_hello_cookie(data: &[u8]) -> Option<&[u8]> {
    const RECORD_HEADER: usize = 13;
    const HANDSHAKE_HEADER: usize = 12;
    if data.len() < RECORD_HEADER + HANDSHAKE_HEADER || data[0] != 0x16 || data[13] != 0x01 {
        return None;
    }
    // Only the fragment at offset 0 carries the start of the body.
    let fragment_offset = u32::from_be_bytes([0, data[19], data[20], data[21]]);
    if fragment_offset != 0 {
        return None;
    }
    let body = RECORD_HEADER + HANDSHAKE_HEADER;
    let session_id_len_at = body + 2 + 32;
    let session_id_len = *data.get(session_id_len_at)? as usize;
    let cookie_len_at = session_id_len_at + 1 + session_id_len;
    let cookie_len = *data.get(cookie_len_at)? as usize;
    if cookie_len == 0 {
        return None;
    }
    data.get(cookie_len_at + 1..cookie_len_at + 1 + cookie_len)
}
