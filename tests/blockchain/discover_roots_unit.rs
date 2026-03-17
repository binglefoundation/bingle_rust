use rust_comms::blockchain::algo_bingle::AlgoBingle;

#[cfg_attr(not(target_os = "ios"), test)]
pub fn discover_roots_parses_relay_ip_from_local_state() {
    // Two accounts, only one has RelayIP
    let accounts = vec![
        "ADDR1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
        "ADDR2BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string(),
    ];
    let app_id = 123u64;

    // Fake getter returns local state for the accounts
    let get_local = |aid: u64, acct: &str| -> Option<Vec<(String, String)>> {
        assert_eq!(aid, app_id);
        match acct {
            "ADDR1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" => Some(vec![
                ("Handle".to_string(), "alice".to_string()),
                ("static_endpoint".to_string(), "127.0.0.1:45000".to_string()),
            ]),
            "ADDR2BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB" => Some(vec![
                ("Handle".to_string(), "bob".to_string()),
            ]),
            _ => None,
        }
    };

    let found = AlgoBingle::discover_root_relays_with(app_id, &accounts, get_local);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, accounts[0]);
    assert_eq!(found[0].1.to_string(), "127.0.0.1:45000");
}
