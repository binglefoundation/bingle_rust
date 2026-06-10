use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use serde_json::json;

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::network_mux_trait::NetworkMux;
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
        ..StartOptions::new("".into())
    };
    let api = BingleApiImpl::new(&opts);
    
    api.access_unsafe_for_tests(|a| {
        a.start(&opts)
    }).expect(&format!("start api for {}", name));
    api
}

#[test]
fn dtls_session_randomness_test() {
    test_util::init_test_logging();
    println!("Starting cross-session randomness test");

    let mut session_ciphertexts = Vec::new();
    let mut session_client_randoms = Vec::new();
    let mut session_server_randoms = Vec::new();

    let session_count = 5;
    let plaintext_payload = "Constant payload for session randomness test".repeat(10);

    for s in 0..session_count {
        println!("--- Session {} ---", s);
        
        let receiver_port = test_util::find_unused_loopback_port();
        let sender_port = test_util::find_unused_loopback_port();
        
        let receiver_api = setup_node(
            &format!("session_test_receiver_{}", s),
            receiver_port,
            test_util::PASSPHRASE_RECEIVE
        );

        let captured_packets: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_packets_clone = captured_packets.clone();

        let mux = receiver_api.engine_mux_for_tests().expect("receiver mux should be available");
        let original_handler = mux.get_handle_dtls().expect("original DTLS handler should be set");
        
        mux.clone().set_handle_dtls_arc(Some(Arc::new(move |source, from, data| {
            captured_packets_clone.lock().expect("lock failed").push(data.to_vec());
            original_handler(source, from, data);
        })));

        let sender_api = setup_node(
            &format!("session_test_sender_{}", s),
            sender_port,
            test_util::PASSPHRASE_SPEND
        );

        // Send one message
        let receiver_id = test_util::ADDRESS_RECEIVE.to_string();
        let receiver_addr = addr(receiver_port);

        let msg = json!({
            "data": plaintext_payload
        });
        
        let ok = sender_api.send_message_to_network(
            &NetworkEndpoint::new_direct(receiver_addr),
            &receiver_id,
            msg,
            None,
        );
        assert!(ok.expect("send_message_to_network should succeed"), "Failed to send message in session {}", s);

        // Wait for completion (handshake + application data)
        std::thread::sleep(Duration::from_secs(2));

        // Shutdown nodes to clean up for next session
        sender_api.access_unsafe_for_tests(|a| a.stop());
        receiver_api.access_unsafe_for_tests(|a| a.stop());

        let packets = captured_packets.lock().expect("lock failed").clone();
        
        // 1. Extract Application Data ciphertext
        let app_data = packets.iter()
            .find(|p| !p.is_empty() && p[0] == 23)
            .expect(&format!("Should have captured Application Data packet in session {}", s));
        
        // Skip DTLS header (13 bytes)
        session_ciphertexts.push(app_data[13..].to_vec());

        // 2. Extract ClientHello.random and ServerHello.random
        let mut client_random = None;
        let mut server_random = None;

        for p in &packets {
            if p.len() > 25 + 32 && p[0] == 22 { // Handshake record
                let handshake_type = p[13];
                if handshake_type == 1 { // ClientHello
                     // Random is at 13 (record) + 12 (handshake) + 2 (version) = 27
                     client_random = Some(p[27..27+32].to_vec());
                } else if handshake_type == 2 { // ServerHello
                     // Random is at 13 (record) + 12 (handshake) + 2 (version) = 27
                     server_random = Some(p[27..27+32].to_vec());
                }
            }
        }
        
        if let Some(cr) = client_random {
            session_client_randoms.push(cr);
        } else {
            println!("Warning: ClientHello random not found in session {}", s);
        }

        if let Some(sr) = server_random {
            session_server_randoms.push(sr);
        } else {
            println!("Warning: ServerHello random not found in session {}", s);
        }
    }

    // Validation
    assert_eq!(session_ciphertexts.len(), session_count, "Did not capture enough session ciphertexts");
    
    for i in 0..session_count {
        for j in i+1..session_count {
            assert!(session_ciphertexts[i] != session_ciphertexts[j], 
                "Identical ciphertexts in session {} and {}!", i, j);
            
            if i < session_client_randoms.len() && j < session_client_randoms.len() {
                assert!(session_client_randoms[i] != session_client_randoms[j],
                    "Identical ClientHello randoms in session {} and {}!", i, j);
            }

            if i < session_server_randoms.len() && j < session_server_randoms.len() {
                assert!(session_server_randoms[i] != session_server_randoms[j],
                    "Identical ServerHello randoms in session {} and {}!", i, j);
            }
        }
    }

    println!("Cross-session randomness test passed! All {} sessions had unique ciphertexts.", session_count);
    if !session_client_randoms.is_empty() {
        println!("Verified {} unique ClientHello randoms.", session_client_randoms.len());
    }
    if !session_server_randoms.is_empty() {
        println!("Verified {} unique ServerHello randoms.", session_server_randoms.len());
    }
}
