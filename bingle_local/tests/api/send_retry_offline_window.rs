// Tests for the recipient-offline window (issue #278): bingle_local::api::send_retry.
// Verifies which failure kinds open the window, and that it opens, lapses and clears per recipient.
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::SendFailureKind;
use bingle_local::api::send_retry::{OfflineWindow, indicates_peer_offline};

const WINDOW: Duration = Duration::from_secs(60);

#[test]
fn peer_unreachable_kinds_indicate_offline() {
    for kind in [
        SendFailureKind::PeerUnreachable,
        SendFailureKind::RelayAllocationFailed,
        SendFailureKind::RecipientNotAdvertised,
        SendFailureKind::NoResponse,
    ] {
        assert!(
            indicates_peer_offline(kind),
            "{kind:?} should open the window"
        );
    }
}

#[test]
fn local_or_permanent_kinds_do_not_indicate_offline() {
    for kind in [
        SendFailureKind::HandleNotFound,
        SendFailureKind::HandleLookupFailed,
        SendFailureKind::InvalidRecipientId,
        SendFailureKind::NoRelayAvailable,
        SendFailureKind::MalformedAdvert,
        SendFailureKind::ProtocolError,
        SendFailureKind::NotReady,
        SendFailureKind::Unknown,
    ] {
        assert!(
            !indicates_peer_offline(kind),
            "{kind:?} should not open the window"
        );
    }
}

#[test]
fn unmarked_recipient_is_not_offline() {
    let window = OfflineWindow::new(WINDOW);
    assert!(!window.is_offline("bob", Instant::now()));
}

#[test]
fn marked_recipient_is_offline_until_the_window_lapses() {
    let mut window = OfflineWindow::new(WINDOW);
    let t0 = Instant::now();
    window.mark_offline("bob", t0);
    assert!(window.is_offline("bob", t0));
    assert!(window.is_offline("bob", t0 + WINDOW - Duration::from_millis(1)));
    assert!(!window.is_offline("bob", t0 + WINDOW));
}

#[test]
fn window_is_per_recipient() {
    let mut window = OfflineWindow::new(WINDOW);
    let t0 = Instant::now();
    window.mark_offline("bob", t0);
    assert!(!window.is_offline("carol", t0));
}

#[test]
fn clear_ends_the_window_early() {
    let mut window = OfflineWindow::new(WINDOW);
    let t0 = Instant::now();
    window.mark_offline("bob", t0);
    window.clear("bob");
    assert!(!window.is_offline("bob", t0));
}

#[test]
fn re_marking_extends_the_window() {
    let mut window = OfflineWindow::new(WINDOW);
    let t0 = Instant::now();
    window.mark_offline("bob", t0);
    let t1 = t0 + Duration::from_secs(30);
    window.mark_offline("bob", t1);
    assert!(window.is_offline("bob", t0 + WINDOW));
    assert!(!window.is_offline("bob", t1 + WINDOW));
}
