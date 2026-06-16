use rust_comms::util::cli_utils::parse_start_options_from_args;
use std::net::SocketAddr;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parse_positional_handle_and_flags() {
    let args = vec![
        "myhandle".to_string(),
        "--passphrase".to_string(), "secret words".to_string(),
        "--relay".to_string(),
    ];
    let opts = parse_start_options_from_args(args).expect("parse ok");
    assert_eq!(opts.handle, "myhandle");
    assert_eq!(opts.algo_passphrase.as_deref(), Some("secret words"));
    assert!(opts.am_relay);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parse_handle_flag_and_static_ip() {
    let args = vec![
        "--handle".to_string(), "bob".to_string(),
        "--static-ip".to_string(), "127.0.0.1:12345".to_string(),
    ];
    let opts = parse_start_options_from_args(args).expect("parse ok");
    assert_eq!(opts.handle, "bob");
    assert_eq!(opts.static_ip, Some("127.0.0.1:12345".parse::<SocketAddr>().unwrap()));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parse_stun_servers_list() {
    let args = vec![
        "alice".to_string(),
        "--stun-servers".to_string(), "1.2.3.4:3478, 5.6.7.8:3478".to_string(),
    ];
    let opts = parse_start_options_from_args(args).expect("parse ok");
    let list = opts.stun_servers.expect("stun_servers present");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0], "1.2.3.4:3478".parse::<SocketAddr>().unwrap());
    assert_eq!(list[1], "5.6.7.8:3478".parse::<SocketAddr>().unwrap());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parse_stun_servers_file() {
    let mut tf = tempfile::NamedTempFile::new().expect("tempfile");
    use std::io::Write;
    writeln!(tf, "10.0.0.1:3478\n10.0.0.2:3478").unwrap();
    let path = tf.path().to_string_lossy().to_string();

    let args = vec![
        "charlie".to_string(),
        "--stun-servers-file".to_string(), path,
    ];
    let opts = parse_start_options_from_args(args).expect("parse ok");
    let list = opts.stun_servers.expect("stun_servers present");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0], "10.0.0.1:3478".parse::<SocketAddr>().unwrap());
    assert_eq!(list[1], "10.0.0.2:3478".parse::<SocketAddr>().unwrap());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn error_on_missing_handle() {
    let args = vec!["--relay".to_string()];
    let err = parse_start_options_from_args(args).unwrap_err();
    assert!(err.contains("Missing handle"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn error_on_invalid_static_ip() {
    let args = vec![
        "dave".to_string(),
        "--static-ip".to_string(), "bad".to_string(),
    ];
    let err = parse_start_options_from_args(args).unwrap_err();
    assert!(err.contains("Invalid --static-ip"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn error_on_unknown_option() {
    let args = vec!["erin".to_string(), "--unknown".to_string()];
    let err = parse_start_options_from_args(args).unwrap_err();
    assert!(err.contains("Unknown option"));
}
