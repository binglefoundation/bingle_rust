// Issue #498: the engine keeps the IPv4 and IPv6 STUN results apart. A change of family is never a
// NAT rebind, and IPv6 is classified on its own with the triangle result (#499) as an input.

use std::net::{Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use bingle_core::api::bingle_api::StartOptions;
use bingle_core::engine::{Engine, Ipv6Triangle, NatType, NatTypeV6};
use bingle_core::stun::StunState;
use bingle_core::util::local_addrs::{LocalAddrSource, LocalIpv6};

use crate::util::reusable_mock_api::{MockApiBoth, to_weak_api_both};

const HOST: Ipv6Addr = Ipv6Addr::new(0x2a01, 0x4f8, 0, 0, 0, 0, 0, 0x42);

/// A fixed view of the host's IPv6 addresses.
struct FixedLocal(LocalIpv6);

impl LocalAddrSource for FixedLocal {
    fn local_ipv6(&self) -> LocalIpv6 {
        self.0.clone()
    }
}

type ListenLog = Arc<Mutex<Vec<(bool, NatType, NatTypeV6)>>>;

fn engine_with_host_v6() -> (Engine, ListenLog) {
    let api = to_weak_api_both(MockApiBoth::new());
    let eng = Engine::new(&StartOptions::new("".into()), api);
    eng.set_local_addrs_for_tests(Arc::new(FixedLocal(LocalIpv6 {
        addrs: vec![Ipv6Addr::LOCALHOST, HOST],
        route: true,
    })));
    let log: ListenLog = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    eng.set_on_listening_handler(Some(Arc::new(move |listening, nat, nat6| {
        sink.lock().unwrap().push((listening, nat, nat6));
    })));
    (eng, log)
}

fn host_mapped() -> SocketAddr {
    SocketAddr::new(HOST.into(), 40000)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_ipv4_to_ipv6_change_is_not_a_rebind() {
    let (eng, log) = engine_with_host_v6();
    let v4: SocketAddr = "1.2.3.4:5000".parse().unwrap();
    eng.set_last_public_addr(Some(v4));
    eng.set_nat_type(NatType::Restricted);

    // Repeat past the IP-change debounce, which would confirm a genuine IPv4 change.
    for _ in 0..5 {
        eng.test_stun_consistent_process_with_addr(host_mapped());
    }

    assert_eq!(eng.last_public_addr(), Some(v4), "IPv4 address untouched");
    assert_eq!(eng.nat_type(), NatType::Restricted, "IPv4 state not reset");
    assert_eq!(eng.last_public_addr6(), Some(host_mapped()));
    assert!(
        log.lock()
            .unwrap()
            .iter()
            .all(|(_, nat, _)| *nat == NatType::Restricted),
        "no reset was reported: {:?}",
        log.lock().unwrap()
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ipv6_is_open_firewalled_until_the_triangle_succeeds() {
    let (eng, log) = engine_with_host_v6();
    eng.test_stun_v6_process(StunState::Consistent, Some(host_mapped()));
    assert_eq!(eng.nat_type_v6(), NatTypeV6::OpenFirewalled);

    eng.set_ipv6_triangle_result(Ipv6Triangle::Succeeded);
    assert_eq!(eng.nat_type_v6(), NatTypeV6::Open);

    let reported: Vec<NatTypeV6> = log.lock().unwrap().iter().map(|(_, _, v6)| *v6).collect();
    assert_eq!(
        reported,
        vec![NatTypeV6::OpenFirewalled, NatTypeV6::Open],
        "each IPv6 change is reported to the app"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_failed_triangle_keeps_ipv6_open_firewalled() {
    let (eng, _log) = engine_with_host_v6();
    eng.test_stun_v6_process(StunState::Single, Some(host_mapped()));
    eng.set_ipv6_triangle_result(Ipv6Triangle::Failed);
    assert_eq!(eng.nat_type_v6(), NatTypeV6::OpenFirewalled);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_new_ipv6_address_forgets_the_triangle_result() {
    let (eng, _log) = engine_with_host_v6();
    eng.test_stun_v6_process(StunState::Consistent, Some(host_mapped()));
    eng.set_ipv6_triangle_result(Ipv6Triangle::Succeeded);
    assert_eq!(eng.nat_type_v6(), NatTypeV6::Open);

    // A privacy-address rotation: still local, but not yet tested.
    let rotated = Ipv6Addr::new(0x2a01, 0x4f8, 0, 0, 0, 0, 0, 0x43);
    eng.set_local_addrs_for_tests(Arc::new(FixedLocal(LocalIpv6 {
        addrs: vec![HOST, rotated],
        route: true,
    })));
    eng.test_stun_v6_process(
        StunState::Consistent,
        Some(SocketAddr::new(rotated.into(), 40000)),
    );
    assert_eq!(eng.nat_type_v6(), NatTypeV6::OpenFirewalled);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ipv4_and_ipv6_states_are_independent() {
    let (eng, log) = engine_with_host_v6();
    eng.set_nat_type(NatType::Restricted);
    eng.notify_listening(true);
    eng.test_stun_v6_process(StunState::Consistent, Some(host_mapped()));
    eng.set_ipv6_triangle_result(Ipv6Triangle::Succeeded);

    assert_eq!(eng.nat_type(), NatType::Restricted);
    assert_eq!(eng.nat_type_v6(), NatTypeV6::Open);
    assert_eq!(
        log.lock().unwrap().last().copied(),
        Some((true, NatType::Restricted, NatTypeV6::Open)),
        "an IPv6-only change is reported with the listening flag unchanged"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ipv6_blocked_is_no_connection_and_no_route_is_none() {
    let (eng, _log) = engine_with_host_v6();
    eng.test_stun_v6_process(StunState::Blocked, None);
    assert_eq!(eng.nat_type_v6(), NatTypeV6::NoConnection);
    assert_eq!(eng.last_public_addr6(), None);

    eng.set_local_addrs_for_tests(Arc::new(FixedLocal(LocalIpv6 {
        addrs: vec![Ipv6Addr::LOCALHOST],
        route: false,
    })));
    eng.test_stun_v6_process(StunState::None, None);
    assert_eq!(eng.nat_type_v6(), NatTypeV6::None);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn stop_forgets_the_ipv6_state() {
    let (eng, _log) = engine_with_host_v6();
    eng.test_stun_v6_process(StunState::Consistent, Some(host_mapped()));
    eng.set_ipv6_triangle_result(Ipv6Triangle::Succeeded);
    eng.stop();
    assert_eq!(eng.nat_type_v6(), NatTypeV6::Unknown);
    assert_eq!(eng.last_public_addr6(), None);
}
