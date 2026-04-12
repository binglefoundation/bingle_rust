use serde_json::json;

/// Helper that mirrors the echo detection logic in bingle_cli's --echo on_message handler.
/// Returns Some(echo_message) if the incoming message is a PlainTextMessage, None otherwise.
fn build_echo_response(message: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(text) = message.get("text").and_then(|v| v.as_str()) {
        let is_plain = message.get("app").map_or(true, |v| v.is_null())
            && message.get("type").map_or(true, |v| v.is_null());
        if is_plain {
            let echo_text = format!("Echo: {}", text);
            return Some(json!({ "text": echo_text }));
        }
    }
    None
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn echo_plain_text_message() {
    let msg = json!({ "text": "Hello" });
    let result = build_echo_response(&msg);
    let echo = result.expect("should produce an echo for plain text");
    assert_eq!(echo["text"], "Echo: Hello");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn echo_plain_text_with_null_app_and_type() {
    let msg = json!({ "text": "Hi there", "app": null, "type": null });
    let result = build_echo_response(&msg);
    let echo = result.expect("should produce an echo when app and type are null");
    assert_eq!(echo["text"], "Echo: Hi there");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn echo_skips_typed_message() {
    let msg = json!({ "text": "Hello", "app": "chat", "type": "markdown" });
    let result = build_echo_response(&msg);
    assert!(result.is_none(), "should not echo a typed message");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn echo_skips_message_with_app_only() {
    let msg = json!({ "text": "Hello", "app": "ping" });
    let result = build_echo_response(&msg);
    assert!(result.is_none(), "should not echo when app is non-null");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn echo_skips_message_without_text() {
    let msg = json!({ "app": "chat", "type": "markdown", "data": {} });
    let result = build_echo_response(&msg);
    assert!(result.is_none(), "should not echo when no text field");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn echo_preserves_original_text() {
    let msg = json!({ "text": "Echo: already echoed" });
    let result = build_echo_response(&msg);
    let echo = result.expect("should still echo");
    assert_eq!(echo["text"], "Echo: Echo: already echoed");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn echo_empty_text() {
    let msg = json!({ "text": "" });
    let result = build_echo_response(&msg);
    let echo = result.expect("should echo even empty text");
    assert_eq!(echo["text"], "Echo: ");
}
