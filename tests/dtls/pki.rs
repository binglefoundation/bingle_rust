use std::path::PathBuf;
use std::process::Command;
use std::fs;
use crate::util::test_util::ADDRESS_RECEIVE;

#[allow(dead_code)]
pub struct TestCerts {
    pub ca_crt: Vec<u8>,
    pub server_crt: Vec<u8>,
    pub server_key: Vec<u8>,
    #[allow(dead_code)]
    pub client_crt: Vec<u8>,
    #[allow(dead_code)]
    pub client_key: Vec<u8>,
}

#[allow(dead_code)]
fn run(cmd: &mut Command) {
    let out = cmd.output().expect("failed to spawn openssl");
    if !out.status.success() {
        panic!(
            "openssl command failed: {:?}\nstdout: {}\nstderr: {}",
            cmd,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[allow(dead_code)]
pub fn generate_ed25519_test_certs() -> TestCerts {
    generate_ed25519_test_certs_with_key(ADDRESS_RECEIVE.to_string().as_str())
}

#[allow(dead_code)]
pub fn generate_ed25519_test_certs_with_key(key: &str) -> TestCerts {
    // Use a temporary directory and read PEMs into memory
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();

    let ca_key = dir.join("ca.key");
    let ca_crt = dir.join("ca.crt");
    let server_key = dir.join("server.key");
    let server_csr = dir.join("server.csr");
    let server_crt = dir.join("server.crt");
    let client_key = dir.join("client.key");
    let client_csr = dir.join("client.csr");
    let client_crt = dir.join("client.crt");

    // CA key (Ed25519) and self-signed certificate
    run(Command::new("openssl").args([
        "genpkey", "-algorithm", "ED25519", "-out",
        path_str(&ca_key)
    ]));
    run(Command::new("openssl").args([
        "req", "-x509", "-key", path_str(&ca_key), "-out", path_str(&ca_crt),
        "-days", "2", "-subj", &format!("/CN=virtual.bingle.home.arpa/O={}", key)
    ]));

    // Server key (ECDSA P-256), CSR, and cert signed by Ed25519 CA
    run(Command::new("openssl").args([
        "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out",
        path_str(&server_key)
    ]));
    run(Command::new("openssl").args([
        "req", "-new", "-key", path_str(&server_key), "-out", path_str(&server_csr),
        "-subj", &format!("/CN={}.", key)
    ]));
    run(Command::new("openssl").args([
        "x509", "-req", "-in", path_str(&server_csr), "-CA", path_str(&ca_crt), "-CAkey",
        path_str(&ca_key), "-CAcreateserial", "-out", path_str(&server_crt), "-days", "2"
    ]));

    // Client key (ECDSA P-256), CSR, and cert signed by Ed25519 CA
    run(Command::new("openssl").args([
        "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out",
        path_str(&client_key)
    ]));
    run(Command::new("openssl").args([
        "req", "-new", "-key", path_str(&client_key), "-out", path_str(&client_csr),
        "-subj", &format!("/CN={}.", key)
    ]));
    run(Command::new("openssl").args([
        "x509", "-req", "-in", path_str(&client_csr), "-CA", path_str(&ca_crt), "-CAkey",
        path_str(&ca_key), "-CAcreateserial", "-out", path_str(&client_crt), "-days", "2"
    ]));

    // Read all PEMs to memory and return
    let read = |p: &PathBuf| fs::read(p).expect("read pem");
    TestCerts {
        ca_crt: read(&ca_crt),
        server_crt: read(&server_crt),
        server_key: read(&server_key),
        client_crt: read(&client_crt),
        client_key: read(&client_key),
    }
}

#[allow(dead_code)]
fn path_str(p: &PathBuf) -> &str {
    // Safe unwrap: temp dir path is valid UTF-8 on supported platforms.
    p.to_str().expect("utf8 path")
}
