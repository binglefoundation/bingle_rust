use crate::api::types::BingleMessage;

/// Callback interface for incoming messages.
///
/// Implement this trait on the React Native / TypeScript side to receive
/// messages as they arrive. Register via `set_message_callback`.
#[uniffi::export(callback_interface)]
pub trait MessageCallback: Send + Sync {
    /// Called when a message is received.
    ///
    /// Parameters:
    /// - sender_id: the Algorand address of the sender
    /// - sender_handle: the handle of the sender
    /// - message: the received message
    fn on_message(&self, sender_id: String, sender_handle: String, message: BingleMessage);
}
