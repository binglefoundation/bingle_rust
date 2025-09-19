#![cfg(not(target_os = "ios"))]

use std::io::{Read, Write};
use std::net::{SocketAddr, UdpSocket};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use rust_comms::dtls::{Dtls, DtlsOpenSsl};
mod pki;

static CLIENT_SEEN: OnceLock<Vec<u8>> = OnceLock::new();

#[test]
fn dtls_openssl_external_s_server_client_send() {
    // Check that openssl CLI is available; if not, skip test gracefully.
    match Command::new("openssl").arg("version").stdout(Stdio::null()).stderr(Stdio::null()).status() {
        Ok(status) if status.success() => {}
        _ => {
            eprintln!("[skipped] openssl CLI not available");
            return;
        }
    }

    // Generate server cert and key (and CA, though we won't require mutual auth)
    let certs = pki::generate_ed25519_test_certs();
    // Touch unused fields to avoid dead_code warnings in this test binary
    let _ = (certs.ca_crt.len(), certs.client_crt.len(), certs.client_key.len());

    // Write server cert and key to temporary files for s_server
    let tmp = tempfile::tempdir().expect("tempdir");
    let server_crt_path = tmp.path().join("server.crt");
    let server_key_path = tmp.path().join("server.key");
    std::fs::write(&server_crt_path, &certs.server_crt).expect("write server cert");
    std::fs::write(&server_key_path, &certs.server_key).expect("write server key");

    // Choose a free UDP port by binding to 127.0.0.1:0 and taking the assigned port.
    let probe = UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe");
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let addr: SocketAddr = SocketAddr::from(([127, 0, 0, 1], port));

    // Spawn openssl s_server for DTLS 1.2
    // Note: -verify 0 to not require client cert; -quiet to keep output minimal.
    let mut child = match Command::new("openssl")
        .arg("s_server")
        .arg("-dtls1_2")
        .arg("-accept").arg(format!("{}:{}", addr.ip(), addr.port()))
        .arg("-cert").arg(server_crt_path.to_string_lossy().to_string())
        .arg("-key").arg(server_key_path.to_string_lossy().to_string())
        .arg("-verify").arg("0")
        .arg("-cipher").arg("eNULL:@SECLEVEL=0")
        .arg("-debug")
        .arg("-msg")
        .arg("-trace")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[skipped] failed to spawn openssl s_server: {}", e);
            return;
        }
    };

    // Prepare to capture and echo stdout/stderr from the openssl command
    let stdout_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let stderr_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let mut join_handles: Vec<thread::JoinHandle<()>> = Vec::new();

    if let Some(mut out) = child.stdout.take() {
        let out_buf = stdout_buf.clone();
        let h = thread::spawn(move || {
            let mut chunk = [0u8; 1024];
            loop {
                match out.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        eprintln!("[openssl stdout] {}", String::from_utf8_lossy(&chunk[..n]));
                        if let Ok(mut b) = out_buf.lock() { b.extend_from_slice(&chunk[..n]); }
                    }
                    Err(e) => {
                        eprintln!("[openssl stdout read err] {}", e);
                        break;
                    }
                }
            }
        });
        join_handles.push(h);
    }
    if let Some(mut err) = child.stderr.take() {
        let err_buf = stderr_buf.clone();
        let h = thread::spawn(move || {
            let mut chunk = [0u8; 1024];
            loop {
                match err.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        eprintln!("[openssl stderr] {}", String::from_utf8_lossy(&chunk[..n]));
                        if let Ok(mut b) = err_buf.lock() { b.extend_from_slice(&chunk[..n]); }
                    }
                    Err(e) => {
                        eprintln!("[openssl stderr read err] {}", e);
                        break;
                    }
                }
            }
        });
        join_handles.push(h);
    }

    // Give the server time to start and bind.
    thread::sleep(Duration::from_millis(400));

    // Keep stdin handle to send data to the DTLS peer via s_server
    let mut child_stdin = child.stdin.take();

    // Step 1: Write payload to openssl s_server stdin so it will send it to the next connected client
    let payload = b"external-openssl-server-test\n";
    if let Some(mut stdin) = child_stdin.take() {
        let _ = stdin.write_all(payload);
        let _ = stdin.flush();
        eprintln!("[openssl stdin] wrote payload: {:?}", std::str::from_utf8(payload).unwrap_or("<non-utf8>"));
    } else {
        eprintln!("[warn] no stdin available to write to openssl s_server");
    }

    // Step 2: Attempt to receive the payload by creating a fresh client that handshakes and reads once
    use std::sync::atomic::{AtomicBool, Ordering};
    static RECEIVED: AtomicBool = AtomicBool::new(false);
    fn capture_handler(_server: &dyn Dtls, _from: &SocketAddr, data: &[u8]) {
        if !data.is_empty() && !RECEIVED.load(Ordering::Relaxed) {
            // store into CLIENT_SEEN only first time
            let _ = CLIENT_SEEN.set(data.to_vec());
            RECEIVED.store(true, Ordering::Relaxed);
        }
    }

    let deadline = Instant::now() + Duration::from_secs(8);
    let mut attempt_ok = false;
    while CLIENT_SEEN.get().is_none() && Instant::now() < deadline {
        let client = DtlsOpenSsl::new().with_null_encryption().with_handle_message(capture_handler);
        if client.send(addr, b"probe").is_ok() { attempt_ok = true; }
        // if not yet received, wait a moment before next attempt
        if CLIENT_SEEN.get().is_some() { break; }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(attempt_ok, "DtlsOpenSsl client failed to establish/read from openssl s_server");

    // Validate that the client handler captured the payload from s_server
    let seen = CLIENT_SEEN.get().cloned().unwrap_or_default();
    assert_eq!(seen.as_slice(), payload, "client did not receive payload from openssl s_server via stdin");

    // Cleanup: try to terminate the server process and join reader threads.
    let _ = child.kill();
    let _ = child.wait();
    for h in join_handles { let _ = h.join(); }
}
