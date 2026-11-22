//! The Sidewinder sidechain seam: the Mailbox (first-in first-out, FIFO) client the
//! store-and-forward path posts to and reads from (store-and-forward epic #200).
//!
//! Story #213 is the foundation: the [`Mailbox`] client wrapper over the Mailbox operations
//! (`post` / `pop`, and `take` where the node has it, Sidewinder #626) and its connection
//! [`MailboxConfig`]. The post-on-delivery-fail and
//! read-on-reconnect wiring are their own stories (#214 / #215).

pub mod mailbox;
pub use mailbox::{
    IDENTITY_REFUSED_BACKOFF, MAILBOX_POP_TYPE, MAILBOX_POST_TYPE, MAILBOX_TAKE_BATCH,
    MAILBOX_TAKE_TYPE, Mailbox, MailboxConfig, PostOutcome, TakeBatch, WatchProgress,
    classify_watch, decode_take_batch, identity_refusal_holds, identity_refusal_message,
    pending_forward_recipients, should_forward_send,
};
#[doc(hidden)]
pub use mailbox::{
    build_pop_request, build_post_request, build_take_request, is_routine_read, map_error,
    next_node_index,
};
