// Issue #498: the STUN finder keeps one state and endpoint per address family, so a dual-stack host
// reports an IPv4 and an IPv6 state instead of reading mixed replies as Inconsistent.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bingle_core::stun::{AddrFamily, StunEndpointFinder, StunEndpointFinderImpl, StunState};

type FamilyLog = Arc<Mutex<Vec<(AddrFamily, StunState, Option<SocketAddr>)>>>;

/// A Binding success response carrying XOR-MAPPED-ADDRESS for `mapped`, in either family.
fn response_for(mapped: SocketAddr) -> Vec<u8> {
    let cookie = [0x21u8, 0x12, 0xA4, 0x42];
    let txid: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let mut value = vec![0x00];
    let xport = (mapped.port() ^ 0x2112).to_be_bytes();
    match mapped.ip() {
        std::net::IpAddr::V4(v4) => {
            value.push(0x01);
            value.extend_from_slice(&xport);
            for (b, m) in v4.octets().iter().zip(cookie) {
                value.push(b ^ m);
            }
        }
        std::net::IpAddr::V6(v6) => {
            value.push(0x02);
            value.extend_from_slice(&xport);
            for (b, m) in v6.octets().iter().zip(cookie.iter().chain(txid.iter())) {
                value.push(b ^ m);
            }
        }
    }
    let mut pkt = vec![0x01, 0x01];
    pkt.extend_from_slice(&((value.len() + 4) as u16).to_be_bytes());
    pkt.extend_from_slice(&cookie);
    pkt.extend_from_slice(&txid);
    pkt.extend_from_slice(&[0x00, 0x20]);
    pkt.extend_from_slice(&(value.len() as u16).to_be_bytes());
    pkt.extend_from_slice(&value);
    pkt
}

fn finder_with(servers: &[SocketAddr]) -> (StunEndpointFinderImpl, FamilyLog) {
    let mut finder = StunEndpointFinderImpl::new();
    finder.init(servers.to_vec(), 100, 200);
    let log: FamilyLog = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    finder.set_state_change_handler(Some(Arc::new(move |family, st, ep| {
        sink.lock().unwrap().push((family, st, ep));
    })));
    (finder, log)
}

/// The last state reported for `family`, if any.
fn last(log: &FamilyLog, family: AddrFamily) -> Option<(StunState, Option<SocketAddr>)> {
    log.lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(f, _, _)| *f == family)
        .map(|(_, st, ep)| (*st, *ep))
}

fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap()
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_ipv4_only_host_reports_ipv4_and_no_ipv6_change() {
    let (s1, s2) = (addr("1.1.1.1:3478"), addr("8.8.8.8:3478"));
    let (mut finder, log) = finder_with(&[s1, s2]);
    let mapped = addr("203.0.113.9:55000");
    finder.process_packet(s1, &response_for(mapped));
    finder.process_packet(s2, &response_for(mapped));

    assert_eq!(
        last(&log, AddrFamily::V4),
        Some((StunState::Consistent, Some(mapped)))
    );
    assert_eq!(last(&log, AddrFamily::V6), None, "IPv6 never left None");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_ipv6_only_host_reports_ipv6_and_no_ipv4_change() {
    let (s1, s2) = (addr("[2606:4700::1]:3478"), addr("[2001:4860::1]:19302"));
    let (mut finder, log) = finder_with(&[s1, s2]);
    let mapped = addr("[2001:db8::42]:40000");
    finder.process_packet(s1, &response_for(mapped));
    assert_eq!(
        last(&log, AddrFamily::V6),
        Some((StunState::Single, Some(mapped)))
    );
    finder.process_packet(s2, &response_for(mapped));

    assert_eq!(
        last(&log, AddrFamily::V6),
        Some((StunState::Consistent, Some(mapped)))
    );
    assert_eq!(last(&log, AddrFamily::V4), None, "IPv4 never left None");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_dual_stack_host_reports_both_families_and_never_inconsistent() {
    let v4s = [addr("1.1.1.1:3478"), addr("8.8.8.8:3478")];
    let v6s = [addr("[2606:4700::1]:3478"), addr("[2001:4860::1]:19302")];
    let all: Vec<SocketAddr> = v4s.iter().chain(v6s.iter()).copied().collect();
    let (mut finder, log) = finder_with(&all);
    let mapped4 = addr("203.0.113.9:55000");
    let mapped6 = addr("[2001:db8::42]:40000");
    // Interleave the families, as replies arrive on a dual-stack socket.
    finder.process_packet(v4s[0], &response_for(mapped4));
    finder.process_packet(v6s[0], &response_for(mapped6));
    finder.process_packet(v4s[1], &response_for(mapped4));
    finder.process_packet(v6s[1], &response_for(mapped6));

    assert_eq!(
        last(&log, AddrFamily::V4),
        Some((StunState::Consistent, Some(mapped4)))
    );
    assert_eq!(
        last(&log, AddrFamily::V6),
        Some((StunState::Consistent, Some(mapped6)))
    );
    assert!(
        !log.lock()
            .unwrap()
            .iter()
            .any(|(_, st, _)| *st == StunState::Inconsistent),
        "mixed-family replies must not read as Inconsistent: {:?}",
        log.lock().unwrap()
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_ipv4_server_reached_through_nat64_counts_towards_ipv4() {
    // A NAT64-synthesised server address is IPv6, but its reply maps an IPv4 address.
    let (s1, s2) = (
        addr("[64:ff9b::101:101]:3478"),
        addr("[64:ff9b::808:808]:3478"),
    );
    let (mut finder, log) = finder_with(&[s1, s2]);
    let mapped = addr("198.51.100.7:61000");
    finder.process_packet(s1, &response_for(mapped));
    finder.process_packet(s2, &response_for(mapped));

    assert_eq!(
        last(&log, AddrFamily::V4),
        Some((StunState::Consistent, Some(mapped)))
    );
    assert_eq!(last(&log, AddrFamily::V6), None);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_inconsistent_family_does_not_disturb_the_other() {
    let v4s = [addr("1.1.1.1:3478"), addr("8.8.8.8:3478")];
    let v6 = addr("[2606:4700::1]:3478");
    let (mut finder, log) = finder_with(&[v4s[0], v4s[1], v6]);
    let mapped6 = addr("[2001:db8::42]:40000");
    finder.process_packet(v6, &response_for(mapped6));
    finder.process_packet(v4s[0], &response_for(addr("203.0.113.9:55000")));
    finder.process_packet(v4s[1], &response_for(addr("203.0.113.9:55001")));

    assert_eq!(
        last(&log, AddrFamily::V4),
        Some((StunState::Inconsistent, None))
    );
    assert_eq!(
        last(&log, AddrFamily::V6),
        Some((StunState::Single, Some(mapped6)))
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn blocked_is_reported_for_both_families() {
    let (finder, log) = finder_with(&[addr("1.1.1.1:3478"), addr("[2606:4700::1]:3478")]);
    // No replies: three search intervals reach Blocked.
    finder.test_ticks(10);

    assert_eq!(last(&log, AddrFamily::V4), Some((StunState::Blocked, None)));
    assert_eq!(last(&log, AddrFamily::V6), Some((StunState::Blocked, None)));
}
