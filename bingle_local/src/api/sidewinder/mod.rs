//! The Sidewinder sidechain seam: the Mailbox (first-in first-out, FIFO) client the
//! store-and-forward path posts to and reads from (store-and-forward epic #200).
//!
//! Story #213 is the foundation: the [`Mailbox`] client wrapper over the two Mailbox operations
//! (`post` / `pop`) and its connection [`MailboxConfig`]. The post-on-delivery-fail and
//! read-on-reconnect wiring are their own stories (#214 / #215).

pub mod mailbox;
pub use mailbox::{MAILBOX_POP_TYPE, MAILBOX_POST_TYPE, Mailbox, MailboxConfig};
#[doc(hidden)]
pub use mailbox::{build_pop_request, build_post_request};
