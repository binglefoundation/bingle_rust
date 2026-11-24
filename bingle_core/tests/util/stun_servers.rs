use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::StartOptions;
use bingle_core::engine::Engine;
use bingle_core::util::stun_servers::{
    DEFAULT_STUN_SERVERS, StunResolver, effective_entries, resolve_stun_entries, split_stun_entries,
};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn split_accepts_hosts_and_both_families_and_strips_comments() {
    let got = split_stun_entries(
        "# comment\nstun.example.com:3478, 192.0.2.1:3478\n[2001:db8::1]:3478 # trailing\n",
    )
    .unwrap();
    assert_eq!(
        got,
        vec![
            "stun.example.com:3478",
            "192.0.2.1:3478",
            "[2001:db8::1]:3478"
        ]
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn split_rejects_bad_entries() {
    for bad in [
        "stun.example.com",
        "stun.example.com:notaport",
        ":3478",
        "2001:db8::1:3478x",
    ] {
        assert!(split_stun_entries(bad).is_err(), "{bad}");
    }
}

struct FakeResolver {
    delay: Duration,
}

impl StunResolver for FakeResolver {
    fn resolve(&self, entry: &str) -> std::io::Result<Vec<SocketAddr>> {
        std::thread::sleep(self.delay);
        match entry {
            "dual.example:3478" => Ok(vec![
                "[2001:db8::1]:3478".parse().unwrap(),
                "192.0.2.1:3478".parse().unwrap(),
            ]),
            "mapped.example:3478" => Ok(vec!["[::ffff:192.0.2.1]:3478".parse().unwrap()]),
            "slow.example:3478" => {
                std::thread::sleep(Duration::from_secs(2));
                Ok(vec!["192.0.2.9:3478".parse().unwrap()])
            }
            _ => Err(std::io::Error::other("no such host")),
        }
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn resolve_keeps_both_families_dedups_and_skips_failures() {
    let entries: Vec<String> = [
        "dual.example:3478",
        "missing.example:3478",
        "mapped.example:3478",
        "198.51.100.7:3478",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let got = resolve_stun_entries(
        &entries,
        Arc::new(FakeResolver {
            delay: Duration::ZERO,
        }),
        Duration::from_secs(1),
    );
    let expected: Vec<SocketAddr> = vec![
        "[2001:db8::1]:3478".parse().unwrap(),
        "192.0.2.1:3478".parse().unwrap(),
        "198.51.100.7:3478".parse().unwrap(),
    ];
    assert_eq!(
        got, expected,
        "mapped duplicate of 192.0.2.1 dropped, failed name skipped"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_hanging_resolver_does_not_block_past_the_deadline() {
    let entries: Vec<String> = vec!["slow.example:3478".into(), "192.0.2.2:3478".into()];
    let start = Instant::now();
    let got = resolve_stun_entries(
        &entries,
        Arc::new(FakeResolver {
            delay: Duration::ZERO,
        }),
        Duration::from_millis(200),
    );
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "returned at the deadline"
    );
    assert_eq!(got, vec!["192.0.2.2:3478".parse::<SocketAddr>().unwrap()]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn names_resolve_in_parallel() {
    let entries: Vec<String> = vec!["dual.example:3478".into(), "mapped.example:3478".into()];
    let start = Instant::now();
    let got = resolve_stun_entries(
        &entries,
        Arc::new(FakeResolver {
            delay: Duration::from_millis(300),
        }),
        Duration::from_secs(2),
    );
    assert!(
        start.elapsed() < Duration::from_millis(550),
        "two 300 ms lookups overlapped"
    );
    assert_eq!(got.len(), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn default_list_is_used_without_a_configured_one() {
    let default: Vec<String> = DEFAULT_STUN_SERVERS.iter().map(|s| s.to_string()).collect();
    assert_eq!(effective_entries(None), default);
    assert_eq!(effective_entries(Some(&[])), default);
    let configured = vec!["192.0.2.1:3478".to_string()];
    assert_eq!(effective_entries(Some(&configured)), configured);
    assert!(DEFAULT_STUN_SERVERS.len() >= 3, "three operators");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn engine_prefers_explicit_addresses_then_stun_hosts() {
    let explicit: SocketAddr = "127.0.0.1:3478".parse().unwrap();
    let opts = StartOptions {
        stun_servers: Some(vec![explicit]),
        stun_hosts: Some(vec!["192.0.2.1:3478".into()]),
        ..StartOptions::new("t".into())
    };
    assert_eq!(Engine::stun_server_addresses(&opts), vec![explicit]);

    // Literal entries resolve without DNS.
    let opts = StartOptions {
        stun_hosts: Some(vec!["192.0.2.1:3478".into(), "[2001:db8::1]:3478".into()]),
        ..StartOptions::new("t".into())
    };
    let expected: Vec<SocketAddr> = vec![
        "192.0.2.1:3478".parse().unwrap(),
        "[2001:db8::1]:3478".parse().unwrap(),
    ];
    assert_eq!(Engine::stun_server_addresses(&opts), expected);
}
