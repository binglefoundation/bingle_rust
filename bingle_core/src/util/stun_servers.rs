//! The STUN server list (issue #496): a built-in default of public hostnames, an optional
//! operator-supplied list that replaces it, and resolution of both into socket addresses in both
//! address families.

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc;
use std::time::Duration;

/// The built-in STUN servers, by hostname, used when no list is supplied. Each operator is listed
/// once: the measured alternatives (2026-11-24) were Cloudflare (documented free service, IPv4 and
/// IPv6), Google (long-standing but undocumented, IPv4 and IPv6; `stun1`–`stun4.l.google.com` are
/// the same server, so only one name counts) and Twilio (documented free service, IPv4 only).
/// NAT classification needs replies from at least two servers at different addresses, so three
/// operators keep it working through one outage.
pub const DEFAULT_STUN_SERVERS: &[&str] = &[
    "stun.cloudflare.com:3478",
    "stun.l.google.com:19302",
    "global.stun.twilio.com:3478",
];

/// How long resolving the whole STUN list may take before the servers resolved so far are used.
pub const STUN_RESOLVE_DEADLINE: Duration = Duration::from_secs(3);

/// Split a STUN list (comma or whitespace separated; `#` starts a comment to end of line) into
/// entries, checking each is `host:port`, `ipv4:port` or `[ipv6]:port`.
pub fn split_stun_entries(s: &str) -> Result<Vec<String>, String> {
    let mut entries = Vec::new();
    for line in s.lines() {
        let line = line.split('#').next().unwrap_or("");
        for part in line.split(|c: char| c == ',' || c.is_whitespace()) {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            check_entry(p)?;
            entries.push(p.to_string());
        }
    }
    Ok(entries)
}

/// Check one entry has a host and a valid port.
fn check_entry(p: &str) -> Result<(), String> {
    if p.parse::<SocketAddr>().is_ok() {
        return Ok(());
    }
    let invalid = || format!("Invalid STUN server entry '{p}': must be <host:port> or <ip:port>");
    let (host, port) = p.rsplit_once(':').ok_or_else(invalid)?;
    if host.is_empty() || host.contains(':') || port.parse::<u16>().is_err() {
        return Err(invalid());
    }
    Ok(())
}

/// Name resolution for STUN hostnames, injectable for tests.
pub trait StunResolver: Send + Sync {
    /// The socket addresses `entry` (`host:port`) resolves to.
    fn resolve(&self, entry: &str) -> std::io::Result<Vec<SocketAddr>>;
}

/// The system resolver (`getaddrinfo` through std).
pub struct SystemStunResolver;

impl StunResolver for SystemStunResolver {
    fn resolve(&self, entry: &str) -> std::io::Result<Vec<SocketAddr>> {
        Ok(entry.to_socket_addrs()?.collect())
    }
}

/// Resolve STUN entries into socket addresses: literals as they are, hostnames through `resolver`,
/// in parallel and within `deadline`. Both address families are kept (an IPv4-mapped result is
/// turned back into plain IPv4), duplicates are dropped keeping first-seen order, and an entry that
/// fails or misses the deadline is skipped with a warning.
pub fn resolve_stun_entries(
    entries: &[String],
    resolver: std::sync::Arc<dyn StunResolver>,
    deadline: Duration,
) -> Vec<SocketAddr> {
    let (tx, rx) = mpsc::channel::<(usize, Vec<SocketAddr>)>();
    let mut pending = 0usize;
    let mut results: Vec<Option<Vec<SocketAddr>>> = vec![None; entries.len()];
    for (i, entry) in entries.iter().enumerate() {
        if let Ok(addr) = entry.parse::<SocketAddr>() {
            results[i] = Some(vec![addr]);
            continue;
        }
        pending += 1;
        let tx = tx.clone();
        let resolver = resolver.clone();
        let entry = entry.clone();
        std::thread::spawn(move || {
            let addrs = match resolver.resolve(&entry) {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!("[stun] could not resolve STUN server {entry}: {e}");
                    Vec::new()
                }
            };
            let _ = tx.send((i, addrs));
        });
    }
    drop(tx);
    let until = std::time::Instant::now() + deadline;
    while pending > 0 {
        let left = until.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok((i, addrs)) => {
                results[i] = Some(addrs);
                pending -= 1;
            }
            Err(_) => {
                tracing::warn!(
                    "[stun] {pending} STUN server name(s) not resolved within {deadline:?}; continuing without them"
                );
                break;
            }
        }
    }
    let mut out: Vec<SocketAddr> = Vec::new();
    for addr in results.into_iter().flatten().flatten() {
        let addr = SocketAddr::new(addr.ip().to_canonical(), addr.port());
        if !out.contains(&addr) {
            out.push(addr);
        }
    }
    out
}

/// The entries to use: `configured` when it has at least one entry, otherwise the built-in list.
pub fn effective_entries(configured: Option<&[String]>) -> Vec<String> {
    match configured {
        Some(c) if !c.is_empty() => c.to_vec(),
        _ => DEFAULT_STUN_SERVERS.iter().map(|s| s.to_string()).collect(),
    }
}
