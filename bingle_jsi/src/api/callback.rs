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

/// Callback interface for log output.
///
/// Implement this trait on the React Native / TypeScript side to receive
/// log messages. Register via `set_log_callback`.
#[uniffi::export(callback_interface)]
pub trait LogCallback: Send + Sync {
    /// Called for each log message.
    ///
    /// Parameters:
    /// - timestamp: milliseconds since Unix epoch
    /// - level: log level string (TRACE, DEBUG, INFO, WARN, ERROR)
    /// - message: the log message text
    fn on_log(&self, timestamp: i64, level: String, message: String);
}

/// Callback interface asking the host to start iOS push registration.
///
/// The APNs device token can only be obtained from the platform (UIKit delivers it to the
/// AppDelegate), so Rust cannot request it directly. When [`request_push_registration`] is called,
/// Rust invokes this callback and the host's thin Swift bridge performs the actual platform calls
/// (`requestAuthorization` + `registerForRemoteNotifications`). The resulting raw token is handed
/// back via [`register_apns_token`]; there is no logic in the bridge.
///
/// Register via `set_push_registration_callback`.
#[uniffi::export(callback_interface)]
pub trait PushRegistrationCallback: Send + Sync {
    /// Called when the host should ask iOS for an APNs device token (permission prompt + register).
    fn on_request_registration(&self);
}

/// Callback interface for listening state changes.
///
/// Implement this trait on the React Native / TypeScript side to receive
/// notifications when the engine starts or stops listening.
/// Register via `set_listening_callback`.
#[uniffi::export(callback_interface)]
pub trait ListeningCallback: Send + Sync {
    /// Called when the listening state changes.
    ///
    /// Parameters:
    /// - listening: true when the engine is listening, false when it stops
    /// - nat_type: the detected NAT type as a string
    ///   ("Unknown", "NoConnection", "Symmetric", "Restricted", "FullCone")
    fn on_listening(&self, listening: bool, nat_type: String);
}
