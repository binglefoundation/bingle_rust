use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::network_mux_trait::NetworkMux;
use rust_comms::engine::BingleAccessUnsafeForTests;

#[path = "../test_util.rs"]
pub mod test_util;

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

fn allocate_udp_port() -> u16 {
    let socket = UdpSocket::bind(addr(0)).expect("bind ephemeral UDP port");
    socket
        .local_addr()
        .expect("read bound ephemeral UDP local addr")
        .port()
}

fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0usize; 256];
    for &byte in data {
        counts[byte as usize] += 1;
    }
    let mut entropy = 0.0;
    let len = data.len() as f64;
    for &count in &counts {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }
    entropy
}

fn setup_node(
    name: &str,
    port: u16,
    passphrase: &str,
    null_encryption: bool,
) -> Arc<BingleApiImpl> {
    let node_addr = addr(port);
    let opts = StartOptions {
        handle: Handle::from(name),
        algo_passphrase: Some(passphrase.to_string()),
        static_ip: Some(node_addr),
        dangerous_debug: null_encryption,
        ..StartOptions::new("".into())
    };
    let api = BingleApiImpl::new(&opts);

    api.access_unsafe_for_tests(|a| {
        if null_encryption {
            a.with_engine_mut(|e| {
                e.with_dtls_mut(|dtls| {
                    dtls.set_null_encryption(true);
                });
            });
        }
        a.start(&opts)
    })
    .expect(&format!("start api for {}", name));
    api
}

fn run_entropy_test(null_encryption: bool, test_name: &str) {
    test_util::init_test_logging();
    println!(
        "Starting entropy test: {} (null_encryption={})",
        test_name, null_encryption
    );

    // 1) Setup receiver
    let receiver_port = allocate_udp_port();
    let receiver_api = setup_node(
        &format!("{}_receiver", test_name),
        receiver_port,
        test_util::PASSPHRASE_RECEIVE,
        null_encryption,
    );

    // Intercept packets on the receiver's mux
    let captured_packets: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_packets_clone = captured_packets.clone();

    let mux = receiver_api
        .engine_mux_for_tests()
        .expect("receiver mux should be available");
    let original_handler = mux
        .get_handle_dtls()
        .expect("original DTLS handler should be set");

    mux.clone()
        .set_handle_dtls_arc(Some(Arc::new(move |source, from, data| {
            // DTLS record type 23 is Application Data
            if !data.is_empty() && data[0] == 23 {
                captured_packets_clone.lock().unwrap().push(data.to_vec());
            }
            original_handler(source, from, data);
        })));

    // 2) Setup sender
    let mut sender_port = allocate_udp_port();
    while sender_port == receiver_port {
        sender_port = allocate_udp_port();
    }
    let sender_api = setup_node(
        &format!("{}_sender", test_name),
        sender_port,
        test_util::PASSPHRASE_SPEND,
        null_encryption,
    );

    // 3) Send 15 messages with payload sized to fit current DATA_SINGLE transport capacity
    let plaintext_payload = "@".repeat(1200);
    let message_count = 15;
    let receiver_id = test_util::ADDRESS_RECEIVE.to_string();
    let receiver_addr = addr(receiver_port);

    for i in 0..message_count {
        let msg = json!({
            "data": plaintext_payload
        });
        let ok = sender_api.send_message_to_network(
            &NetworkEndpoint::new_direct(receiver_addr),
            &receiver_id,
            msg,
            None,
        );
        assert!(
            ok.expect("send_message_to_network should succeed"),
            "Failed to send message {}",
            i
        );
        // Small delay to allow processing
        std::thread::sleep(Duration::from_millis(100));
    }

    // 4) Analyze captured packets
    // Wait a bit for all packets to be received
    std::thread::sleep(Duration::from_secs(5));

    let all_packets = captured_packets.lock().unwrap().clone();
    // Filter for our messages based on size (1200 payload + JSON overhead + DTLS overhead).
    // Keep bounds narrow enough to exclude cert-announce DTLS application records.
    let packets: Vec<Vec<u8>> = all_packets
        .into_iter()
        .filter(|p| p.len() > 1200 && p.len() < 1500)
        .collect();

    println!(
        "Analyzed {}/{} DTLS application data packets",
        packets.len(),
        captured_packets.lock().unwrap().len()
    );
    assert!(
        packets.len() >= message_count,
        "Should have captured at least {} target packets, got {}",
        message_count,
        packets.len()
    );

    let mut total_entropy = 0.0;
    let mut identical_payloads = 0;

    for (i, p1) in packets.iter().enumerate() {
        // Skip DTLS header (13 bytes for DTLS 1.2)
        let encrypted_payload = &p1[13..];
        let entropy = calculate_entropy(encrypted_payload);
        total_entropy += entropy;

        if null_encryption {
            let payload_preview = String::from_utf8_lossy(encrypted_payload);
            let preview_chars: String = payload_preview.chars().take(100).collect();
            println!(
                "Packet {} ({} bytes): entropy = {:.4}, preview: {}",
                i,
                p1.len(),
                entropy,
                preview_chars
            );
        } else {
            println!(
                "Packet {} ({} bytes): entropy = {:.4}",
                i,
                p1.len(),
                entropy
            );
        }

        if null_encryption {
            // Null encryption should have very low entropy (all '@' signs)
            assert!(
                entropy < 1.0,
                "Entropy too low for null encryption packet {}: {:.4}",
                i,
                entropy
            );
        } else {
            // Encrypted data should have high entropy (close to 8.0)
            assert!(
                entropy > 7.5,
                "Entropy too low for packet {}: {:.4}",
                i,
                entropy
            );
        }

        // Compare with other packets (payload only, skipping DTLS header which has seq num)
        for (j, p2) in packets.iter().enumerate() {
            if i != j && p1[13..] == p2[13..] {
                identical_payloads += 1;
            }
        }
    }

    let avg_entropy = total_entropy / packets.len() as f64;
    println!("Average entropy: {:.4}", avg_entropy);

    if null_encryption {
        assert!(
            avg_entropy < 1.0,
            "Average entropy too high for null encryption: {:.4}",
            avg_entropy
        );
        // Note: identical_payloads might still be 0 if a sequence-dependent MAC is used even in null encryption.
        // We primarily rely on the low entropy check.
    } else {
        assert!(
            avg_entropy > 7.8,
            "Average entropy too low: {:.4}",
            avg_entropy
        );
        assert_eq!(identical_payloads, 0, "Found identical encrypted payloads!");
    }

    println!("DTLS entropy validation passed for {}!", test_name);
}

#[test]
fn dtls_encryption_randomness_test() {
    run_entropy_test(false, "randomness");
}

#[test]
fn dtls_encryption_null_test() {
    run_entropy_test(true, "null_encryption");
}
