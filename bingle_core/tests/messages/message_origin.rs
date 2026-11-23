use std::cell::Cell;

use bingle_core::messages::router::{only_from_relay, origin_allowed};

use crate::util::message_mocks::all_message_samples;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_only_messages_need_a_relay_sender() {
    for (label, msg) in all_message_samples()
        .into_iter()
        .filter(|(_, m)| only_from_relay(m))
    {
        assert!(
            !origin_allowed(&msg, "CLIENT", |_| false),
            "{label} accepted from a non-relay"
        );
        assert!(
            origin_allowed(&msg, "RELAY", |_| true),
            "{label} refused from a relay"
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn other_messages_are_accepted_without_a_relay_check() {
    for (label, msg) in all_message_samples()
        .into_iter()
        .filter(|(_, m)| !only_from_relay(m))
    {
        let asked = Cell::new(false);
        assert!(
            origin_allowed(&msg, "CLIENT", |_| {
                asked.set(true);
                false
            }),
            "{label} refused"
        );
        assert!(
            !asked.get(),
            "{label} needlessly checked whether the sender is a relay"
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_relay_check_is_asked_about_the_sender() {
    let (_, msg) = all_message_samples()
        .into_iter()
        .find(|(_, m)| only_from_relay(m))
        .expect("a relay-only sample");
    let asked_about = std::cell::RefCell::new(String::new());
    origin_allowed(&msg, "SENDER123", |id| {
        *asked_about.borrow_mut() = id.to_string();
        true
    });
    assert_eq!(asked_about.borrow().as_str(), "SENDER123");
}
