use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use serde_json::json;

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::BingleAccessUnsafeForTests;

#[path = "../test_util.rs"]
pub mod test_util;

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

fn setup_node(name: &str, port: u16, passphrase: &str) -> Arc<BingleApiImpl> {
    let node_addr = addr(port);
    let opts = StartOptions {
        handle: Handle::from(name),
        algo_passphrase: Some(passphrase.to_string()),
        static_ip: Some(node_addr),
        dangerous_debug: false,
        ..Default::default()
    };
    let api = BingleApiImpl::new(&opts);
    
    api.access_unsafe_for_tests(|a| {
        a.start(&opts)
    }).expect(&format!("start api for {}", name));
    api
}

#[test]
fn dtls_unregistered_identity_is_rejected_in_engine() {
    test_util::init_test_logging();
    println!("Starting DTLS identity validation test");

    // 1) Setup receiver (server)
    let receiver_port = test_util::find_unused_loopback_port();
    let receiver_api = setup_node(
        "identity_receiver",
        receiver_port,
        test_util::PASSPHRASE_RECEIVE
    );

    // Track received messages on the receiver
    let received_messages = Arc::new(Mutex::new(Vec::new()));
    let received_messages_clone = received_messages.clone();
    receiver_api.access_unsafe_for_tests(|a| {
        a.set_on_message(Some(Arc::new(move |_user_id, _handle, message| {
            received_messages_clone.lock().unwrap().push(message);
        })));
    });

    // 2) Setup sender (client)
    let sender_port = test_util::find_unused_loopback_port();
    let sender_api = setup_node(
        "identity_sender",
        sender_port,
        test_util::PASSPHRASE_SPEND
    );
    let _sender_id = test_util::ADDRESS_SPEND.to_string();

    // 3) Initially, the receiver will accept the message (default behavior currently)
    // because it doesn't have a mock and will either hit real blockchain (fails/timeout) or it's currently not checking at all.
    // Wait, if it hits the real blockchain it might return None.
    
    // Configure mock on receiver to REJECT the sender's ID
    receiver_api.set_id_to_handle_lookup_mock_for_tests(Box::new(move |id| {
        if id == &test_util::ADDRESS_SPEND.to_string() {
            println!("Mock: Rejecting identity {}", id);
            Ok(None)
        } else {
            Ok(Some(Handle::from("other")))
        }
    }));

    // 4) Send message
    let payload = json!({ "hello": "world" });
    let ok = sender_api.send_message_to_network(
        &NetworkEndpoint::new_direct(addr(receiver_port)),
        &test_util::ADDRESS_RECEIVE.to_string(),
        payload.clone(),
        None,
    );
    assert!(ok, "Failed to send message");

    // 5) Verify message is NOT received
    std::thread::sleep(Duration::from_secs(2));
    {
        let msgs = received_messages.lock().unwrap();
        assert_eq!(msgs.len(), 0, "Message should NOT have been received because identity was rejected by mock");
    }

    // 6) Now configure mock to ACCEPT the sender's ID
    receiver_api.set_id_to_handle_lookup_mock_for_tests(Box::new(move |id| {
        if id == &test_util::ADDRESS_SPEND.to_string() {
            println!("Mock: Accepting identity {}", id);
            Ok(Some(Handle::from("valid_user")))
        } else {
            Ok(None)
        }
    }));

    // 7) Send another message
    let ok = sender_api.send_message_to_network(
        &NetworkEndpoint::new_direct(addr(receiver_port)),
        &test_util::ADDRESS_RECEIVE.to_string(),
        payload,
        None,
    );
    assert!(ok, "Failed to send second message");

    // 8) Verify message IS received
    std::thread::sleep(Duration::from_secs(2));
    {
        let msgs = received_messages.lock().unwrap();
        assert!(msgs.len() > 0, "Message SHOULD have been received because identity was accepted by mock");
    }

    println!("DTLS identity validation test passed!");
}
