// Unit tests for the pure REPL pieces (bingle_cli::chat_repl).
use bingle_cli::chat_repl::{ChatInput, CurrentRecipient, parse_input};
use bingle_cli::chat_send::SendTarget;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parse_input_classifies_lines() {
    assert_eq!(parse_input(""), ChatInput::Empty);
    assert_eq!(parse_input("   "), ChatInput::Empty);
    assert_eq!(parse_input("!exit"), ChatInput::Exit);
    assert_eq!(parse_input("  !exit \n"), ChatInput::Exit);
    assert_eq!(
        parse_input("hello there\n"),
        ChatInput::Send {
            text: "hello there".to_string()
        }
    );
    assert_eq!(
        parse_input("/echo\n"),
        ChatInput::Switch {
            prefix: "echo".to_string()
        }
    );
    assert_eq!(
        parse_input("/  echo-test-2  "),
        ChatInput::Switch {
            prefix: "echo-test-2".to_string()
        }
    );
    // A bare slash is a switch with an empty prefix (cmd_chat prints usage).
    assert_eq!(
        parse_input("/"),
        ChatInput::Switch {
            prefix: String::new()
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn recipient_seed_from_args() {
    assert_eq!(
        CurrentRecipient::from_args(Some("bob"), None),
        CurrentRecipient::Handle {
            handle: "bob".to_string(),
            id: None
        }
    );
    assert_eq!(
        CurrentRecipient::from_args(None, Some("PEER_ID")),
        CurrentRecipient::Id("PEER_ID".to_string())
    );
    assert_eq!(
        CurrentRecipient::from_args(None, None),
        CurrentRecipient::None
    );
    // Empty strings are treated as absent.
    assert_eq!(
        CurrentRecipient::from_args(Some(""), Some("")),
        CurrentRecipient::None
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn prompt_reflects_recipient() {
    assert_eq!(CurrentRecipient::None.prompt(), "User? > ");
    assert_eq!(
        CurrentRecipient::Handle {
            handle: "echo-test-1".to_string(),
            id: None
        }
        .prompt(),
        "[echo-test-1] > "
    );
    assert_eq!(
        CurrentRecipient::Id("PEER_ID".to_string()).prompt(),
        "[PEER_ID] > "
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn target_maps_recipient() {
    assert_eq!(CurrentRecipient::None.target(), None);
    assert_eq!(
        CurrentRecipient::Handle {
            handle: "bob".to_string(),
            id: Some("ID".to_string())
        }
        .target(),
        Some(SendTarget::Handle("bob".to_string()))
    );
    assert_eq!(
        CurrentRecipient::Id("ID".to_string()).target(),
        Some(SendTarget::Id("ID".to_string()))
    );
}
