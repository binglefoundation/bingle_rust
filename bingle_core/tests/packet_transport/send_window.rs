//! Sender-side acknowledgement window for fragmented sends (spec §5.3, issue #518).

use bingle_core::packet_transport::send_window::SendWindow;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_window_spans_64_fragments_from_the_first_unacknowledged() {
    let mut w = SendWindow::new(200);
    assert_eq!(w.lowest_unacked(), 0);
    assert_eq!(w.window_end(), 64);
    assert!(!w.any_ack());

    // Fragments 0..=3 and 5 acknowledged: the window starts at the hole, fragment 4.
    assert!(w.apply_ack_window(0, 0b10_1111));
    assert_eq!(w.lowest_unacked(), 4);
    assert_eq!(w.window_end(), 68);
    assert_eq!(w.acked_count(), 5);
    assert!(w.any_ack());
    assert_eq!(
        w.unacked_in_window(68),
        vec![4].into_iter().chain(6..68).collect::<Vec<_>>()
    );
    assert_eq!(w.unacked_in_window(10), vec![4, 6, 7, 8, 9]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn repeated_acknowledgement_is_not_progress() {
    let mut w = SendWindow::new(10);
    assert!(w.apply_ack_window(0, 0b11));
    let progress = w.progress();
    assert!(!w.apply_ack_window(0, 0b11));
    assert_eq!(w.progress(), progress);
    assert_eq!(w.acked_count(), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn bits_past_the_last_fragment_are_ignored() {
    let mut w = SendWindow::new(65);
    assert!(w.apply_ack_window(64, u64::MAX));
    assert_eq!(w.acked_count(), 1);
    assert!(w.is_acked(64));
    assert!(!w.is_acked(65));
    assert!(!w.apply_ack_window(60_000, u64::MAX));
    assert_eq!(w.window_end(), 64);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn acknowledging_everything_moves_the_window_to_the_end() {
    let mut w = SendWindow::new(130);
    w.apply_ack_window(0, u64::MAX);
    w.apply_ack_window(64, u64::MAX);
    w.apply_ack_window(128, 0b11);
    assert_eq!(w.lowest_unacked(), 130);
    assert_eq!(w.window_end(), 130);
    assert!(w.unacked_in_window(130).is_empty());
    assert!(!w.is_complete());
    w.mark_complete();
    assert!(w.is_complete());
}
