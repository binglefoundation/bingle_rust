use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

use bingle_core::util::cli_utils::parse_start_options_from_args;

fn write_temp_file(content: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    p.push(format!("stun-{}-{}.txt", pid, now));
    fs::write(&p, content).expect("failed to write temp stunservers file");
    p
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parse_stun_file_ignores_hash_comments_and_blank_lines() {
    // Using TEST-NET IP ranges to avoid DNS: direct SocketAddr parse only
    let file = write_temp_file(
        "# leading comment\n\n  192.0.2.1:3478   # inline comment after entry\n# full-line comment\n198.51.100.2:3478,   203.0.113.3:3478  # mixed separators and trailing comment\n\n",
    );

    let args = vec![
        "--handle".into(),
        "tester".into(),
        "--stun-servers-file".into(),
        file.to_string_lossy().to_string(),
    ];

    let opts = parse_start_options_from_args(args).expect("should parse args");
    let list = opts
        .stun_servers
        .as_ref()
        .expect("stun servers should be present");

    // Validate exact set and order of parsed addresses
    let expected: Vec<SocketAddr> = vec![
        "192.0.2.1:3478".parse().unwrap(),
        "198.51.100.2:3478".parse().unwrap(),
        "203.0.113.3:3478".parse().unwrap(),
    ];
    assert_eq!(list, &expected);
}
