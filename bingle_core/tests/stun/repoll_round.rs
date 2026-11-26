// Issue #549: on a dual-stack host, a STUN re-poll clears every server's answer before sending, so
// the first reply of the new round used to leave the other family with no answers and report it as
// None. The engine reads an IPv4 None as STUN gone silent and drops its registration, so a
// dual-stack host re-registered on every re-poll. A settled family now keeps its state until its
// round has enough replies to decide, and a family that does not answer for a whole round goes to
// None at the next poll.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap()
}

/// Two IPv4 and two IPv6 servers, and the endpoint each family maps to.
struct DualStack {
    finder: StunEndpointFinderImpl,
    log: FamilyLog,
    v4s: [SocketAddr; 2],
    v6s: [SocketAddr; 2],
    mapped4: SocketAddr,
    mapped6: SocketAddr,
}

impl DualStack {
    /// A finder that searches every tick and re-polls every `repeat_ticks` once settled.
    fn new(repeat_ticks: u64, silence: Option<Duration>) -> DualStack {
        let v4s = [addr("1.1.1.1:3478"), addr("8.8.8.8:3478")];
        let v6s = [addr("[2606:4700::1]:3478"), addr("[2001:4860::1]:19302")];
        let mut finder = StunEndpointFinderImpl::new();
        if let Some(t) = silence {
            finder.set_no_response_timeout_for_tests(t);
        }
        let all: Vec<SocketAddr> = v4s.iter().chain(v6s.iter()).copied().collect();
        finder.init(all, 100, repeat_ticks * 100);
        let log: FamilyLog = Arc::new(Mutex::new(Vec::new()));
        let sink = log.clone();
        finder.set_state_change_handler(Some(Arc::new(move |family, st, ep| {
            sink.lock().unwrap().push((family, st, ep));
        })));
        DualStack {
            finder,
            log,
            v4s,
            v6s,
            mapped4: addr("203.0.113.9:55000"),
            mapped6: addr("[2001:db8::42]:40000"),
        }
    }

    /// Search, answer from all four servers, and return once both families are Consistent.
    fn settled(repeat_ticks: u64, silence: Option<Duration>) -> DualStack {
        let mut d = DualStack::new(repeat_ticks, silence);
        d.finder.test_ticks(1);
        d.reply_all();
        assert_eq!(
            d.last(AddrFamily::V4),
            Some((StunState::Consistent, Some(d.mapped4)))
        );
        assert_eq!(
            d.last(AddrFamily::V6),
            Some((StunState::Consistent, Some(d.mapped6)))
        );
        d.log.lock().unwrap().clear();
        d
    }

    fn reply_v4(&mut self, i: usize) {
        self.finder
            .process_packet(self.v4s[i], &response_for(self.mapped4));
    }

    fn reply_v6(&mut self, i: usize) {
        self.finder
            .process_packet(self.v6s[i], &response_for(self.mapped6));
    }

    fn reply_all(&mut self) {
        self.reply_v4(0);
        self.reply_v6(0);
        self.reply_v4(1);
        self.reply_v6(1);
    }

    fn last(&self, family: AddrFamily) -> Option<(StunState, Option<SocketAddr>)> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(f, _, _)| *f == family)
            .map(|(_, st, ep)| (*st, *ep))
    }

    fn events(&self) -> Vec<(AddrFamily, StunState, Option<SocketAddr>)> {
        self.log.lock().unwrap().clone()
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_ipv6_reply_first_in_a_re_poll_does_not_report_ipv4() {
    let mut d = DualStack::settled(2, None);
    d.finder.test_ticks(2); // re-poll: every server's answer is cleared
    d.reply_v6(0);
    assert_eq!(
        d.last(AddrFamily::V4),
        None,
        "IPv4 must not change: {:?}",
        d.events()
    );
    d.reply_v6(1);
    d.reply_v4(0);
    d.reply_v4(1);
    assert!(
        d.events().is_empty(),
        "a re-poll that sees the same endpoints reports nothing: {:?}",
        d.events()
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_ipv4_reply_first_in_a_re_poll_does_not_report_ipv6() {
    let mut d = DualStack::settled(2, None);
    d.finder.test_ticks(2);
    d.reply_v4(0);
    assert_eq!(
        d.last(AddrFamily::V6),
        None,
        "IPv6 must not change: {:?}",
        d.events()
    );
    d.reply_v4(1);
    d.reply_v6(0);
    d.reply_v6(1);
    assert!(d.events().is_empty(), "{:?}", d.events());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn re_polls_stay_quiet_round_after_round() {
    let mut d = DualStack::settled(2, None);
    for _ in 0..5 {
        d.finder.test_ticks(2);
        d.reply_v6(0);
        d.reply_v4(0);
        d.reply_v6(1);
        d.reply_v4(1);
    }
    assert!(d.events().is_empty(), "{:?}", d.events());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_nat_mapping_change_is_reported_once_the_round_agrees() {
    let mut d = DualStack::settled(2, None);
    d.finder.test_ticks(2);
    d.mapped4 = addr("203.0.113.9:55001");
    d.reply_v6(0);
    d.reply_v4(0);
    assert_eq!(
        d.last(AddrFamily::V4),
        None,
        "one reply does not decide: {:?}",
        d.events()
    );
    d.reply_v4(1);
    assert_eq!(
        d.last(AddrFamily::V4),
        Some((StunState::Consistent, Some(d.mapped4)))
    );
    d.reply_v6(1);
    assert_eq!(d.last(AddrFamily::V6), None, "{:?}", d.events());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_family_that_stops_answering_goes_to_none_at_the_next_poll() {
    let mut d = DualStack::settled(2, None);
    d.finder.test_ticks(2);
    // IPv6 goes away: only the IPv4 servers answer this round.
    d.reply_v4(0);
    d.reply_v4(1);
    assert_eq!(
        d.last(AddrFamily::V6),
        None,
        "not before the round ends: {:?}",
        d.events()
    );
    d.finder.test_ticks(2); // the next poll closes the round
    assert_eq!(d.last(AddrFamily::V6), Some((StunState::None, None)));
    assert_eq!(
        d.last(AddrFamily::V4),
        None,
        "IPv4 kept answering: {:?}",
        d.events()
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_silence_timeout_still_reports_none_for_both_families() {
    // Re-poll rarely, so the silence timeout comes first.
    let d = DualStack::settled(50, Some(Duration::from_millis(300)));
    d.finder.test_ticks(50); // re-poll; nothing answers
    d.finder.test_ticks(10);
    // Searching on with no answers goes on to Blocked; None must come first, for both families.
    let events = d.events();
    for family in [AddrFamily::V4, AddrFamily::V6] {
        assert_eq!(
            events.iter().find(|(f, _, _)| *f == family),
            Some(&(family, StunState::None, None)),
            "{:?}",
            events
        );
    }
}
