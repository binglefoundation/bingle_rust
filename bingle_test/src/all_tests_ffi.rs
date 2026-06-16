
#[cfg(target_os = "ios")]
pub fn run_named_test(name: &str) -> bool {
    use std::panic;
    match name {
        "api::bingle_api_impl_integration::relay_check_end_to_end_on_message_receives_response" => {
            match panic::catch_unwind(|| crate::api::bingle_api_impl_integration::relay_check_end_to_end_on_message_receives_response()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::bingle_api_impl_integration::relay_check_end_to_end_on_message_receives_response panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::bingle_api_impl_integration::relay_check_end_to_end_on_message_receives_response panicked: {}", s); }
                    else { tracing::error!("Test api::bingle_api_impl_integration::relay_check_end_to_end_on_message_receives_response panicked with unknown error"); }
                    false
                }
            }
        },
        "api::bingle_api_impl_integration::send_message_to_network_without_addr_fails_gracefully" => {
            match panic::catch_unwind(|| crate::api::bingle_api_impl_integration::send_message_to_network_without_addr_fails_gracefully()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::bingle_api_impl_integration::send_message_to_network_without_addr_fails_gracefully panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::bingle_api_impl_integration::send_message_to_network_without_addr_fails_gracefully panicked: {}", s); }
                    else { tracing::error!("Test api::bingle_api_impl_integration::send_message_to_network_without_addr_fails_gracefully panicked with unknown error"); }
                    false
                }
            }
        },
        "api::bingle_api_impl_integration::start_succeeds" => {
            match panic::catch_unwind(|| crate::api::bingle_api_impl_integration::start_succeeds()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::bingle_api_impl_integration::start_succeeds panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::bingle_api_impl_integration::start_succeeds panicked: {}", s); }
                    else { tracing::error!("Test api::bingle_api_impl_integration::start_succeeds panicked with unknown error"); }
                    false
                }
            }
        },
        "api::bingle_api_impl_unit::start_sets_issuer_and_passes_to_dtls_send" => {
            match panic::catch_unwind(|| crate::api::bingle_api_impl_unit::start_sets_issuer_and_passes_to_dtls_send()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::bingle_api_impl_unit::start_sets_issuer_and_passes_to_dtls_send panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::bingle_api_impl_unit::start_sets_issuer_and_passes_to_dtls_send panicked: {}", s); }
                    else { tracing::error!("Test api::bingle_api_impl_unit::start_sets_issuer_and_passes_to_dtls_send panicked with unknown error"); }
                    false
                }
            }
        },
        "api::bingle_api_impl_unit::unit_send_message_to_network_calls_dtls_send" => {
            match panic::catch_unwind(|| crate::api::bingle_api_impl_unit::unit_send_message_to_network_calls_dtls_send()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::bingle_api_impl_unit::unit_send_message_to_network_calls_dtls_send panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::bingle_api_impl_unit::unit_send_message_to_network_calls_dtls_send panicked: {}", s); }
                    else { tracing::error!("Test api::bingle_api_impl_unit::unit_send_message_to_network_calls_dtls_send panicked with unknown error"); }
                    false
                }
            }
        },
        "api::bingle_api_relay_check_two_nodes::bingle_api_relay_check_two_nodes" => {
            match panic::catch_unwind(|| crate::api::bingle_api_relay_check_two_nodes::bingle_api_relay_check_two_nodes()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::bingle_api_relay_check_two_nodes::bingle_api_relay_check_two_nodes panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::bingle_api_relay_check_two_nodes::bingle_api_relay_check_two_nodes panicked: {}", s); }
                    else { tracing::error!("Test api::bingle_api_relay_check_two_nodes::bingle_api_relay_check_two_nodes panicked with unknown error"); }
                    false
                }
            }
        },
        "api::bingle_api_relay_dtls::bingle_api_send_via_relay" => {
            match panic::catch_unwind(|| crate::api::bingle_api_relay_dtls::bingle_api_send_via_relay()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::bingle_api_relay_dtls::bingle_api_send_via_relay panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::bingle_api_relay_dtls::bingle_api_send_via_relay panicked: {}", s); }
                    else { tracing::error!("Test api::bingle_api_relay_dtls::bingle_api_send_via_relay panicked with unknown error"); }
                    false
                }
            }
        },
        "api::bingle_api_start_fail::start_returns_err_on_invalid_passphrase" => {
            match panic::catch_unwind(|| crate::api::bingle_api_start_fail::start_returns_err_on_invalid_passphrase()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::bingle_api_start_fail::start_returns_err_on_invalid_passphrase panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::bingle_api_start_fail::start_returns_err_on_invalid_passphrase panicked: {}", s); }
                    else { tracing::error!("Test api::bingle_api_start_fail::start_returns_err_on_invalid_passphrase panicked with unknown error"); }
                    false
                }
            }
        },
        "api::bingle_getters::getters_after_start" => {
            match panic::catch_unwind(|| crate::api::bingle_getters::getters_after_start()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::bingle_getters::getters_after_start panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::bingle_getters::getters_after_start panicked: {}", s); }
                    else { tracing::error!("Test api::bingle_getters::getters_after_start panicked with unknown error"); }
                    false
                }
            }
        },
        "api::bingle_getters::getters_default_none" => {
            match panic::catch_unwind(|| crate::api::bingle_getters::getters_default_none()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::bingle_getters::getters_default_none panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::bingle_getters::getters_default_none panicked: {}", s); }
                    else { tracing::error!("Test api::bingle_getters::getters_default_none panicked with unknown error"); }
                    false
                }
            }
        },
        "api::dtls_via_relay_integration::dtls_send_via_relay_end_to_end" => {
            match panic::catch_unwind(|| crate::api::dtls_via_relay_integration::dtls_send_via_relay_end_to_end()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::dtls_via_relay_integration::dtls_send_via_relay_end_to_end panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::dtls_via_relay_integration::dtls_send_via_relay_end_to_end panicked: {}", s); }
                    else { tracing::error!("Test api::dtls_via_relay_integration::dtls_send_via_relay_end_to_end panicked with unknown error"); }
                    false
                }
            }
        },
        "api::endpoint_identify_integration::bingle_api_endpoint_identify_via_forced_stun" => {
            match panic::catch_unwind(|| crate::api::endpoint_identify_integration::bingle_api_endpoint_identify_via_forced_stun()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::endpoint_identify_integration::bingle_api_endpoint_identify_via_forced_stun panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::endpoint_identify_integration::bingle_api_endpoint_identify_via_forced_stun panicked: {}", s); }
                    else { tracing::error!("Test api::endpoint_identify_integration::bingle_api_endpoint_identify_via_forced_stun panicked with unknown error"); }
                    false
                }
            }
        },
        "api::network_endpoint_key::direct_endpoint_key_has_only_inet_addr" => {
            match panic::catch_unwind(|| crate::api::network_endpoint_key::direct_endpoint_key_has_only_inet_addr()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::network_endpoint_key::direct_endpoint_key_has_only_inet_addr panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::network_endpoint_key::direct_endpoint_key_has_only_inet_addr panicked: {}", s); }
                    else { tracing::error!("Test api::network_endpoint_key::direct_endpoint_key_has_only_inet_addr panicked with unknown error"); }
                    false
                }
            }
        },
        "api::network_endpoint_key::relay_endpoint_key_contains_both_id_and_channel" => {
            match panic::catch_unwind(|| crate::api::network_endpoint_key::relay_endpoint_key_contains_both_id_and_channel()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::network_endpoint_key::relay_endpoint_key_contains_both_id_and_channel panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::network_endpoint_key::relay_endpoint_key_contains_both_id_and_channel panicked: {}", s); }
                    else { tracing::error!("Test api::network_endpoint_key::relay_endpoint_key_contains_both_id_and_channel panicked with unknown error"); }
                    false
                }
            }
        },
        "api::network_endpoint_key::relay_endpoint_key_panics_if_channel_missing" => { panic::catch_unwind(|| crate::api::network_endpoint_key::relay_endpoint_key_panics_if_channel_missing()).is_err() },
        "api::network_endpoint_key::relay_keys_with_same_id_and_diff_channel_are_distinct" => {
            match panic::catch_unwind(|| crate::api::network_endpoint_key::relay_keys_with_same_id_and_diff_channel_are_distinct()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::network_endpoint_key::relay_keys_with_same_id_and_diff_channel_are_distinct panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::network_endpoint_key::relay_keys_with_same_id_and_diff_channel_are_distinct panicked: {}", s); }
                    else { tracing::error!("Test api::network_endpoint_key::relay_keys_with_same_id_and_diff_channel_are_distinct panicked with unknown error"); }
                    false
                }
            }
        },
        "api::on_listening_handler::on_listening_handler_creates_and_deletes_sentinel" => {
            match panic::catch_unwind(|| crate::api::on_listening_handler::on_listening_handler_creates_and_deletes_sentinel()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::on_listening_handler::on_listening_handler_creates_and_deletes_sentinel panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::on_listening_handler::on_listening_handler_creates_and_deletes_sentinel panicked: {}", s); }
                    else { tracing::error!("Test api::on_listening_handler::on_listening_handler_creates_and_deletes_sentinel panicked with unknown error"); }
                    false
                }
            }
        },
        "api::pki_generate_pki_from_ops::generate_pki_from_ops_produces_valid_chain_and_expected_cns" => {
            match panic::catch_unwind(|| crate::api::pki_generate_pki_from_ops::generate_pki_from_ops_produces_valid_chain_and_expected_cns()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test api::pki_generate_pki_from_ops::generate_pki_from_ops_produces_valid_chain_and_expected_cns panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test api::pki_generate_pki_from_ops::generate_pki_from_ops_produces_valid_chain_and_expected_cns panicked: {}", s); }
                    else { tracing::error!("Test api::pki_generate_pki_from_ops::generate_pki_from_ops_produces_valid_chain_and_expected_cns panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::algo_bingle_unit::algo_bingle_param_validation" => {
            match panic::catch_unwind(|| crate::blockchain::algo_bingle_unit::algo_bingle_param_validation()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::algo_bingle_unit::algo_bingle_param_validation panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::algo_bingle_unit::algo_bingle_param_validation panicked: {}", s); }
                    else { tracing::error!("Test blockchain::algo_bingle_unit::algo_bingle_param_validation panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::algo_change_reserve_unit::change_reserve_errors_on_invalid_reserve_address" => {
            match panic::catch_unwind(|| crate::blockchain::algo_change_reserve_unit::change_reserve_errors_on_invalid_reserve_address()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::algo_change_reserve_unit::change_reserve_errors_on_invalid_reserve_address panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::algo_change_reserve_unit::change_reserve_errors_on_invalid_reserve_address panicked: {}", s); }
                    else { tracing::error!("Test blockchain::algo_change_reserve_unit::change_reserve_errors_on_invalid_reserve_address panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::algo_change_reserve_unit::change_reserve_errors_on_zero_asset_id" => {
            match panic::catch_unwind(|| crate::blockchain::algo_change_reserve_unit::change_reserve_errors_on_zero_asset_id()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::algo_change_reserve_unit::change_reserve_errors_on_zero_asset_id panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::algo_change_reserve_unit::change_reserve_errors_on_zero_asset_id panicked: {}", s); }
                    else { tracing::error!("Test blockchain::algo_change_reserve_unit::change_reserve_errors_on_zero_asset_id panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::algo_ops_address_derivation_test::derives_address_from_legacy_b64_seed_when_constructed" => {
            match panic::catch_unwind(|| crate::blockchain::algo_ops_address_derivation_test::derives_address_from_legacy_b64_seed_when_constructed()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::algo_ops_address_derivation_test::derives_address_from_legacy_b64_seed_when_constructed panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::algo_ops_address_derivation_test::derives_address_from_legacy_b64_seed_when_constructed panicked: {}", s); }
                    else { tracing::error!("Test blockchain::algo_ops_address_derivation_test::derives_address_from_legacy_b64_seed_when_constructed panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::algo_ops_address_derivation_test::derives_address_from_mnemonic_when_constructed" => {
            match panic::catch_unwind(|| crate::blockchain::algo_ops_address_derivation_test::derives_address_from_mnemonic_when_constructed()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::algo_ops_address_derivation_test::derives_address_from_mnemonic_when_constructed panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::algo_ops_address_derivation_test::derives_address_from_mnemonic_when_constructed panicked: {}", s); }
                    else { tracing::error!("Test blockchain::algo_ops_address_derivation_test::derives_address_from_mnemonic_when_constructed panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::algo_ops_more_test::algo_ops_more_suite" => {
            match panic::catch_unwind(|| crate::blockchain::algo_ops_more_test::algo_ops_more_suite()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::algo_ops_more_test::algo_ops_more_suite panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::algo_ops_more_test::algo_ops_more_suite panicked: {}", s); }
                    else { tracing::error!("Test blockchain::algo_ops_more_test::algo_ops_more_suite panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::algo_ops_reserve_helpers::test_parse_creator_reserve_from_asset_info_value_variants" => {
            match panic::catch_unwind(|| crate::blockchain::algo_ops_reserve_helpers::test_parse_creator_reserve_from_asset_info_value_variants()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::algo_ops_reserve_helpers::test_parse_creator_reserve_from_asset_info_value_variants panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::algo_ops_reserve_helpers::test_parse_creator_reserve_from_asset_info_value_variants panicked: {}", s); }
                    else { tracing::error!("Test blockchain::algo_ops_reserve_helpers::test_parse_creator_reserve_from_asset_info_value_variants panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::algo_ops_reserve_helpers::test_parse_holding_amount_from_account_value" => {
            match panic::catch_unwind(|| crate::blockchain::algo_ops_reserve_helpers::test_parse_holding_amount_from_account_value()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::algo_ops_reserve_helpers::test_parse_holding_amount_from_account_value panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::algo_ops_reserve_helpers::test_parse_holding_amount_from_account_value panicked: {}", s); }
                    else { tracing::error!("Test blockchain::algo_ops_reserve_helpers::test_parse_holding_amount_from_account_value panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::algo_ops_test::algo_ops_basic_suite" => {
            match panic::catch_unwind(|| crate::blockchain::algo_ops_test::algo_ops_basic_suite()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::algo_ops_test::algo_ops_basic_suite panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::algo_ops_test::algo_ops_basic_suite panicked: {}", s); }
                    else { tracing::error!("Test blockchain::algo_ops_test::algo_ops_basic_suite panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::asset_ops_test::asset_ops_suite" => {
            match panic::catch_unwind(|| crate::blockchain::asset_ops_test::asset_ops_suite()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::asset_ops_test::asset_ops_suite panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::asset_ops_test::asset_ops_suite panicked: {}", s); }
                    else { tracing::error!("Test blockchain::asset_ops_test::asset_ops_suite panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::dapp_app_integration::deploy_call_validate_and_delete_teal_app" => {
            match panic::catch_unwind(|| crate::blockchain::dapp_app_integration::deploy_call_validate_and_delete_teal_app()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::dapp_app_integration::deploy_call_validate_and_delete_teal_app panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::dapp_app_integration::deploy_call_validate_and_delete_teal_app panicked: {}", s); }
                    else { tracing::error!("Test blockchain::dapp_app_integration::deploy_call_validate_and_delete_teal_app panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::get_bingle_price::test_extract_bingle_price_missing" => {
            match panic::catch_unwind(|| crate::blockchain::get_bingle_price::test_extract_bingle_price_missing()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::get_bingle_price::test_extract_bingle_price_missing panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::get_bingle_price::test_extract_bingle_price_missing panicked: {}", s); }
                    else { tracing::error!("Test blockchain::get_bingle_price::test_extract_bingle_price_missing panicked with unknown error"); }
                    false
                }
            }
        },
        "blockchain::get_bingle_price::test_extract_bingle_price_ok" => {
            match panic::catch_unwind(|| crate::blockchain::get_bingle_price::test_extract_bingle_price_ok()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test blockchain::get_bingle_price::test_extract_bingle_price_ok panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test blockchain::get_bingle_price::test_extract_bingle_price_ok panicked: {}", s); }
                    else { tracing::error!("Test blockchain::get_bingle_price::test_extract_bingle_price_ok panicked with unknown error"); }
                    false
                }
            }
        },
        "cli::run_args::parse_run_args_with_positional_handle" => {
            match panic::catch_unwind(|| crate::cli::run_args::parse_run_args_with_positional_handle()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test cli::run_args::parse_run_args_with_positional_handle panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test cli::run_args::parse_run_args_with_positional_handle panicked: {}", s); }
                    else { tracing::error!("Test cli::run_args::parse_run_args_with_positional_handle panicked with unknown error"); }
                    false
                }
            }
        },
        "ddb::advert_record_json::advert_record_serde_roundtrip" => {
            match panic::catch_unwind(|| crate::ddb::advert_record_json::advert_record_serde_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test ddb::advert_record_json::advert_record_serde_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test ddb::advert_record_json::advert_record_serde_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test ddb::advert_record_json::advert_record_serde_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "ddb::backend::delete_then_lookup_none" => {
            match panic::catch_unwind(|| crate::ddb::backend::delete_then_lookup_none()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test ddb::backend::delete_then_lookup_none panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test ddb::backend::delete_then_lookup_none panicked: {}", s); }
                    else { tracing::error!("Test ddb::backend::delete_then_lookup_none panicked with unknown error"); }
                    false
                }
            }
        },
        "ddb::backend::upsert_then_lookup_returns_same_record" => {
            match panic::catch_unwind(|| crate::ddb::backend::upsert_then_lookup_returns_same_record()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test ddb::backend::upsert_then_lookup_returns_same_record panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test ddb::backend::upsert_then_lookup_returns_same_record panicked: {}", s); }
                    else { tracing::error!("Test ddb::backend::upsert_then_lookup_returns_same_record panicked with unknown error"); }
                    false
                }
            }
        },
        "ddb::backend::upsert_updates_existing" => {
            match panic::catch_unwind(|| crate::ddb::backend::upsert_updates_existing()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test ddb::backend::upsert_updates_existing panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test ddb::backend::upsert_updates_existing panicked: {}", s); }
                    else { tracing::error!("Test ddb::backend::upsert_updates_existing panicked with unknown error"); }
                    false
                }
            }
        },
        "ddb::ddb_client_lookup::ddb_client_lookup_returns_endpoint" => {
            match panic::catch_unwind(|| crate::ddb::ddb_client_lookup::ddb_client_lookup_returns_endpoint()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test ddb::ddb_client_lookup::ddb_client_lookup_returns_endpoint panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test ddb::ddb_client_lookup::ddb_client_lookup_returns_endpoint panicked: {}", s); }
                    else { tracing::error!("Test ddb::ddb_client_lookup::ddb_client_lookup_returns_endpoint panicked with unknown error"); }
                    false
                }
            }
        },
        "ddb::ddb_client_register_ip::ddb_client_register_ip_ok" => {
            match panic::catch_unwind(|| crate::ddb::ddb_client_register_ip::ddb_client_register_ip_ok()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test ddb::ddb_client_register_ip::ddb_client_register_ip_ok panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test ddb::ddb_client_register_ip::ddb_client_register_ip_ok panicked: {}", s); }
                    else { tracing::error!("Test ddb::ddb_client_register_ip::ddb_client_register_ip_ok panicked with unknown error"); }
                    false
                }
            }
        },
        "ddb::ddb_client_register_relay::ddb_client_register_relay_ok_and_persisted" => {
            match panic::catch_unwind(|| crate::ddb::ddb_client_register_relay::ddb_client_register_relay_ok_and_persisted()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test ddb::ddb_client_register_relay::ddb_client_register_relay_ok_and_persisted panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test ddb::ddb_client_register_relay::ddb_client_register_relay_ok_and_persisted panicked: {}", s); }
                    else { tracing::error!("Test ddb::ddb_client_register_relay::ddb_client_register_relay_ok_and_persisted panicked with unknown error"); }
                    false
                }
            }
        },
        "distributed_mutex::basic::unit_acquire_returns_value" => {
            match panic::catch_unwind(|| crate::distributed_mutex::basic::unit_acquire_returns_value()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test distributed_mutex::basic::unit_acquire_returns_value panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test distributed_mutex::basic::unit_acquire_returns_value panicked: {}", s); }
                    else { tracing::error!("Test distributed_mutex::basic::unit_acquire_returns_value panicked with unknown error"); }
                    false
                }
            }
        },
        "distributed_mutex::basic::unit_exclusive_execution_across_threads" => {
            match panic::catch_unwind(|| crate::distributed_mutex::basic::unit_exclusive_execution_across_threads()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test distributed_mutex::basic::unit_exclusive_execution_across_threads panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test distributed_mutex::basic::unit_exclusive_execution_across_threads panicked: {}", s); }
                    else { tracing::error!("Test distributed_mutex::basic::unit_exclusive_execution_across_threads panicked with unknown error"); }
                    false
                }
            }
        },
        "distributed_mutex::dynamic_add::modified_lamport_dynamic_add_node_after_start" => {
            match panic::catch_unwind(|| crate::distributed_mutex::dynamic_add::modified_lamport_dynamic_add_node_after_start()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test distributed_mutex::dynamic_add::modified_lamport_dynamic_add_node_after_start panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test distributed_mutex::dynamic_add::modified_lamport_dynamic_add_node_after_start panicked: {}", s); }
                    else { tracing::error!("Test distributed_mutex::dynamic_add::modified_lamport_dynamic_add_node_after_start panicked with unknown error"); }
                    false
                }
            }
        },
        "distributed_mutex::islanding::modified_lamport_partitioned_networks_no_dual_hold_c_and_d" => {
            match panic::catch_unwind(|| crate::distributed_mutex::islanding::modified_lamport_partitioned_networks_no_dual_hold_c_and_d()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test distributed_mutex::islanding::modified_lamport_partitioned_networks_no_dual_hold_c_and_d panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test distributed_mutex::islanding::modified_lamport_partitioned_networks_no_dual_hold_c_and_d panicked: {}", s); }
                    else { tracing::error!("Test distributed_mutex::islanding::modified_lamport_partitioned_networks_no_dual_hold_c_and_d panicked with unknown error"); }
                    false
                }
            }
        },
        "distributed_mutex::modified_lamport::modified_lamport_majority_with_one_down" => {
            match panic::catch_unwind(|| crate::distributed_mutex::modified_lamport::modified_lamport_majority_with_one_down()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test distributed_mutex::modified_lamport::modified_lamport_majority_with_one_down panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test distributed_mutex::modified_lamport::modified_lamport_majority_with_one_down panicked: {}", s); }
                    else { tracing::error!("Test distributed_mutex::modified_lamport::modified_lamport_majority_with_one_down panicked with unknown error"); }
                    false
                }
            }
        },
        "distributed_mutex::modified_lamport::modified_lamport_mutual_exclusion_3_nodes" => {
            match panic::catch_unwind(|| crate::distributed_mutex::modified_lamport::modified_lamport_mutual_exclusion_3_nodes()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test distributed_mutex::modified_lamport::modified_lamport_mutual_exclusion_3_nodes panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test distributed_mutex::modified_lamport::modified_lamport_mutual_exclusion_3_nodes panicked: {}", s); }
                    else { tracing::error!("Test distributed_mutex::modified_lamport::modified_lamport_mutual_exclusion_3_nodes panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_client_echo_roundtrip::dtls_client_echo_roundtrip" => {
            match panic::catch_unwind(|| crate::dtls::dtls_client_echo_roundtrip::dtls_client_echo_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_client_echo_roundtrip::dtls_client_echo_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_client_echo_roundtrip::dtls_client_echo_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_client_echo_roundtrip::dtls_client_echo_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_client_keeps_stream_open::dtls_client_keeps_stream_open_across_sends" => {
            match panic::catch_unwind(|| crate::dtls::dtls_client_keeps_stream_open::dtls_client_keeps_stream_open_across_sends()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_client_keeps_stream_open::dtls_client_keeps_stream_open_across_sends panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_client_keeps_stream_open::dtls_client_keeps_stream_open_across_sends panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_client_keeps_stream_open::dtls_client_keeps_stream_open_across_sends panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_debug_alert::dtls_debug_includes_alert_level_and_description" => {
            match panic::catch_unwind(|| crate::dtls::dtls_debug_alert::dtls_debug_includes_alert_level_and_description()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_debug_alert::dtls_debug_includes_alert_level_and_description panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_debug_alert::dtls_debug_includes_alert_level_and_description panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_debug_alert::dtls_debug_includes_alert_level_and_description panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_debug_sequence::dtls_debug_compact_includes_sequence_and_epoch" => {
            match panic::catch_unwind(|| crate::dtls::dtls_debug_sequence::dtls_debug_compact_includes_sequence_and_epoch()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_debug_sequence::dtls_debug_compact_includes_sequence_and_epoch panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_debug_sequence::dtls_debug_compact_includes_sequence_and_epoch panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_debug_sequence::dtls_debug_compact_includes_sequence_and_epoch panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_debug_sequence::dtls_trace_json_includes_sequence_and_epoch" => {
            match panic::catch_unwind(|| crate::dtls::dtls_debug_sequence::dtls_trace_json_includes_sequence_and_epoch()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_debug_sequence::dtls_trace_json_includes_sequence_and_epoch panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_debug_sequence::dtls_trace_json_includes_sequence_and_epoch panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_debug_sequence::dtls_trace_json_includes_sequence_and_epoch panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_external_openssl_server::dtls_openssl_external_s_server_client_send" => {
            match panic::catch_unwind(|| crate::dtls::dtls_external_openssl_server::dtls_openssl_external_s_server_client_send()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_external_openssl_server::dtls_openssl_external_s_server_client_send panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_external_openssl_server::dtls_openssl_external_s_server_client_send panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_external_openssl_server::dtls_openssl_external_s_server_client_send panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_loopback_e2e::dtls_openssl_end_to_end_loopback_echo" => {
            match panic::catch_unwind(|| crate::dtls::dtls_loopback_e2e::dtls_openssl_end_to_end_loopback_echo()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_loopback_e2e::dtls_openssl_end_to_end_loopback_echo panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_loopback_e2e::dtls_openssl_end_to_end_loopback_echo panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_loopback_e2e::dtls_openssl_end_to_end_loopback_echo panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_multi_client_loopback_e2e::dtls_openssl_multi_client_loopback_echo" => {
            match panic::catch_unwind(|| crate::dtls::dtls_multi_client_loopback_e2e::dtls_openssl_multi_client_loopback_echo()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_multi_client_loopback_e2e::dtls_openssl_multi_client_loopback_echo panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_multi_client_loopback_e2e::dtls_openssl_multi_client_loopback_echo panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_multi_client_loopback_e2e::dtls_openssl_multi_client_loopback_echo panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_openssl_smoke::dtls_openssl_udp_listener_invokes_handler" => {
            match panic::catch_unwind(|| crate::dtls::dtls_openssl_smoke::dtls_openssl_udp_listener_invokes_handler()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_openssl_smoke::dtls_openssl_udp_listener_invokes_handler panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_openssl_smoke::dtls_openssl_udp_listener_invokes_handler panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_openssl_smoke::dtls_openssl_udp_listener_invokes_handler panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_peer_certificate_handlers::dtls_openssl_peer_certificate_handlers_are_invoked" => {
            match panic::catch_unwind(|| crate::dtls::dtls_peer_certificate_handlers::dtls_openssl_peer_certificate_handlers_are_invoked()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_peer_certificate_handlers::dtls_openssl_peer_certificate_handlers_are_invoked panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_peer_certificate_handlers::dtls_openssl_peer_certificate_handlers_are_invoked panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_peer_certificate_handlers::dtls_openssl_peer_certificate_handlers_are_invoked panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_server_peer_cert_rejection::dtls_openssl_server_rejects_client_when_peer_cert_handler_fails" => {
            match panic::catch_unwind(|| crate::dtls::dtls_server_peer_cert_rejection::dtls_openssl_server_rejects_client_when_peer_cert_handler_fails()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_server_peer_cert_rejection::dtls_openssl_server_rejects_client_when_peer_cert_handler_fails panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_server_peer_cert_rejection::dtls_openssl_server_rejects_client_when_peer_cert_handler_fails panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_server_peer_cert_rejection::dtls_openssl_server_rejects_client_when_peer_cert_handler_fails panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_start_with_network_mux::dtls_start_accepts_external_network_mux_udp" => {
            match panic::catch_unwind(|| crate::dtls::dtls_start_with_network_mux::dtls_start_accepts_external_network_mux_udp()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_start_with_network_mux::dtls_start_accepts_external_network_mux_udp panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_start_with_network_mux::dtls_start_accepts_external_network_mux_udp panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_start_with_network_mux::dtls_start_accepts_external_network_mux_udp panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::dtls_stun_interleave_handshake::stun_response_does_not_interfere_with_dtls_flow" => {
            match panic::catch_unwind(|| crate::dtls::dtls_stun_interleave_handshake::stun_response_does_not_interfere_with_dtls_flow()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::dtls_stun_interleave_handshake::stun_response_does_not_interfere_with_dtls_flow panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::dtls_stun_interleave_handshake::stun_response_does_not_interfere_with_dtls_flow panicked: {}", s); }
                    else { tracing::error!("Test dtls::dtls_stun_interleave_handshake::stun_response_does_not_interfere_with_dtls_flow panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::network_mux_udp_reprocess::reprocess_dispatches_and_enqueues_dtls" => {
            match panic::catch_unwind(|| crate::dtls::network_mux_udp_reprocess::reprocess_dispatches_and_enqueues_dtls()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::network_mux_udp_reprocess::reprocess_dispatches_and_enqueues_dtls panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::network_mux_udp_reprocess::reprocess_dispatches_and_enqueues_dtls panicked: {}", s); }
                    else { tracing::error!("Test dtls::network_mux_udp_reprocess::reprocess_dispatches_and_enqueues_dtls panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::network_mux_udp_tests::dispatches_stun_dtls_turn" => {
            match panic::catch_unwind(|| crate::dtls::network_mux_udp_tests::dispatches_stun_dtls_turn()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::network_mux_udp_tests::dispatches_stun_dtls_turn panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::network_mux_udp_tests::dispatches_stun_dtls_turn panicked: {}", s); }
                    else { tracing::error!("Test dtls::network_mux_udp_tests::dispatches_stun_dtls_turn panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::network_mux_udp_tests::ignores_zrtp_rtp_unknown" => {
            match panic::catch_unwind(|| crate::dtls::network_mux_udp_tests::ignores_zrtp_rtp_unknown()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::network_mux_udp_tests::ignores_zrtp_rtp_unknown panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::network_mux_udp_tests::ignores_zrtp_rtp_unknown panicked: {}", s); }
                    else { tracing::error!("Test dtls::network_mux_udp_tests::ignores_zrtp_rtp_unknown panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::network_mux_udp_tests::write_relay_wraps_payload_in_turn_channel_data" => {
            match panic::catch_unwind(|| crate::dtls::network_mux_udp_tests::write_relay_wraps_payload_in_turn_channel_data()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::network_mux_udp_tests::write_relay_wraps_payload_in_turn_channel_data panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::network_mux_udp_tests::write_relay_wraps_payload_in_turn_channel_data panicked: {}", s); }
                    else { tracing::error!("Test dtls::network_mux_udp_tests::write_relay_wraps_payload_in_turn_channel_data panicked with unknown error"); }
                    false
                }
            }
        },
        "dtls::network_mux_udp_tests::write_sends_payload" => {
            match panic::catch_unwind(|| crate::dtls::network_mux_udp_tests::write_sends_payload()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test dtls::network_mux_udp_tests::write_sends_payload panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test dtls::network_mux_udp_tests::write_sends_payload panicked: {}", s); }
                    else { tracing::error!("Test dtls::network_mux_udp_tests::write_sends_payload panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::ddb_client_non_optional::bingle_api_impl_exposes_non_optional_engine_ddb_client" => {
            match panic::catch_unwind(|| crate::engine::ddb_client_non_optional::bingle_api_impl_exposes_non_optional_engine_ddb_client()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::ddb_client_non_optional::bingle_api_impl_exposes_non_optional_engine_ddb_client panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::ddb_client_non_optional::bingle_api_impl_exposes_non_optional_engine_ddb_client panicked: {}", s); }
                    else { tracing::error!("Test engine::ddb_client_non_optional::bingle_api_impl_exposes_non_optional_engine_ddb_client panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::ddb_client_non_optional::engine_new_has_non_optional_ddb_client" => {
            match panic::catch_unwind(|| crate::engine::ddb_client_non_optional::engine_new_has_non_optional_ddb_client()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::ddb_client_non_optional::engine_new_has_non_optional_ddb_client panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::ddb_client_non_optional::engine_new_has_non_optional_ddb_client panicked: {}", s); }
                    else { tracing::error!("Test engine::ddb_client_non_optional::engine_new_has_non_optional_ddb_client panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::ddb_upsert::ddb_upsert_ignored_when_not_relay" => {
            match panic::catch_unwind(|| crate::engine::ddb_upsert::ddb_upsert_ignored_when_not_relay()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::ddb_upsert::ddb_upsert_ignored_when_not_relay panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::ddb_upsert::ddb_upsert_ignored_when_not_relay panicked: {}", s); }
                    else { tracing::error!("Test engine::ddb_upsert::ddb_upsert_ignored_when_not_relay panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::ddb_upsert::ddb_upsert_rejected_on_id_mismatch" => {
            match panic::catch_unwind(|| crate::engine::ddb_upsert::ddb_upsert_rejected_on_id_mismatch()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::ddb_upsert::ddb_upsert_rejected_on_id_mismatch panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::ddb_upsert::ddb_upsert_rejected_on_id_mismatch panicked: {}", s); }
                    else { tracing::error!("Test engine::ddb_upsert::ddb_upsert_rejected_on_id_mismatch panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::ddb_upsert::ddb_upsert_success_when_server_is_relay" => {
            match panic::catch_unwind(|| crate::engine::ddb_upsert::ddb_upsert_success_when_server_is_relay()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::ddb_upsert::ddb_upsert_success_when_server_is_relay panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::ddb_upsert::ddb_upsert_success_when_server_is_relay panicked: {}", s); }
                    else { tracing::error!("Test engine::ddb_upsert::ddb_upsert_success_when_server_is_relay panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::dtls_send_no_lazy_start::engine_dtls_send_without_start_fails" => {
            match panic::catch_unwind(|| crate::engine::dtls_send_no_lazy_start::engine_dtls_send_without_start_fails()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::dtls_send_no_lazy_start::engine_dtls_send_without_start_fails panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::dtls_send_no_lazy_start::engine_dtls_send_without_start_fails panicked: {}", s); }
                    else { tracing::error!("Test engine::dtls_send_no_lazy_start::engine_dtls_send_without_start_fails panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::engine_bind_unspecified_ip::engine_binds_to_unspecified_ip_when_static_addr_is_provided" => {
            match panic::catch_unwind(|| crate::engine::engine_bind_unspecified_ip::engine_binds_to_unspecified_ip_when_static_addr_is_provided()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::engine_bind_unspecified_ip::engine_binds_to_unspecified_ip_when_static_addr_is_provided panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::engine_bind_unspecified_ip::engine_binds_to_unspecified_ip_when_static_addr_is_provided panicked: {}", s); }
                    else { tracing::error!("Test engine::engine_bind_unspecified_ip::engine_binds_to_unspecified_ip_when_static_addr_is_provided panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::engine_bingle_dtls_basic::engine_basic_bingle_dtls_layer" => {
            match panic::catch_unwind(|| crate::engine::engine_bingle_dtls_basic::engine_basic_bingle_dtls_layer()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::engine_bingle_dtls_basic::engine_basic_bingle_dtls_layer panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::engine_bingle_dtls_basic::engine_basic_bingle_dtls_layer panicked: {}", s); }
                    else { tracing::error!("Test engine::engine_bingle_dtls_basic::engine_basic_bingle_dtls_layer panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::engine_connections::engine_send_to_peer_tracks_connections_and_reuses" => {
            match panic::catch_unwind(|| crate::engine::engine_connections::engine_send_to_peer_tracks_connections_and_reuses()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::engine_connections::engine_send_to_peer_tracks_connections_and_reuses panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::engine_connections::engine_send_to_peer_tracks_connections_and_reuses panicked: {}", s); }
                    else { tracing::error!("Test engine::engine_connections::engine_send_to_peer_tracks_connections_and_reuses panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::seen_endpoints::engine_tracks_seen_endpoints" => {
            match panic::catch_unwind(|| crate::engine::seen_endpoints::engine_tracks_seen_endpoints()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::seen_endpoints::engine_tracks_seen_endpoints panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::seen_endpoints::engine_tracks_seen_endpoints panicked: {}", s); }
                    else { tracing::error!("Test engine::seen_endpoints::engine_tracks_seen_endpoints panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::turn_relay_forwards_dtls::end_to_end_turn_relay_forwards_dtls" => {
            match panic::catch_unwind(|| crate::engine::turn_relay_forwards_dtls::end_to_end_turn_relay_forwards_dtls()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::turn_relay_forwards_dtls::end_to_end_turn_relay_forwards_dtls panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::turn_relay_forwards_dtls::end_to_end_turn_relay_forwards_dtls panicked: {}", s); }
                    else { tracing::error!("Test engine::turn_relay_forwards_dtls::end_to_end_turn_relay_forwards_dtls panicked with unknown error"); }
                    false
                }
            }
        },
        "engine::turn_relay_integration::end_to_end_turn_relay_forwards_payload" => {
            match panic::catch_unwind(|| crate::engine::turn_relay_integration::end_to_end_turn_relay_forwards_payload()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test engine::turn_relay_integration::end_to_end_turn_relay_forwards_payload panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test engine::turn_relay_integration::end_to_end_turn_relay_forwards_payload panicked: {}", s); }
                    else { tracing::error!("Test engine::turn_relay_integration::end_to_end_turn_relay_forwards_payload panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::ddb_messages_json::ddb_get_epoch_and_info_roundtrip" => {
            match panic::catch_unwind(|| crate::messages::ddb_messages_json::ddb_get_epoch_and_info_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::ddb_messages_json::ddb_get_epoch_and_info_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::ddb_messages_json::ddb_get_epoch_and_info_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test messages::ddb_messages_json::ddb_get_epoch_and_info_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::ddb_messages_json::ddb_init_and_dump_roundtrip" => {
            match panic::catch_unwind(|| crate::messages::ddb_messages_json::ddb_init_and_dump_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::ddb_messages_json::ddb_init_and_dump_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::ddb_messages_json::ddb_init_and_dump_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test messages::ddb_messages_json::ddb_init_and_dump_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::ddb_messages_json::ddb_query_and_response_roundtrip" => {
            match panic::catch_unwind(|| crate::messages::ddb_messages_json::ddb_query_and_response_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::ddb_messages_json::ddb_query_and_response_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::ddb_messages_json::ddb_query_and_response_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test messages::ddb_messages_json::ddb_query_and_response_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::ddb_messages_json::ddb_signon_and_response_roundtrip" => {
            match panic::catch_unwind(|| crate::messages::ddb_messages_json::ddb_signon_and_response_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::ddb_messages_json::ddb_signon_and_response_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::ddb_messages_json::ddb_signon_and_response_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test messages::ddb_messages_json::ddb_signon_and_response_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::ddb_messages_json::ddb_update_and_delete_roundtrip" => {
            match panic::catch_unwind(|| crate::messages::ddb_messages_json::ddb_update_and_delete_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::ddb_messages_json::ddb_update_and_delete_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::ddb_messages_json::ddb_update_and_delete_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test messages::ddb_messages_json::ddb_update_and_delete_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::ddb_messages_json::ddb_upsert_serde_roundtrip" => {
            match panic::catch_unwind(|| crate::messages::ddb_messages_json::ddb_upsert_serde_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::ddb_messages_json::ddb_upsert_serde_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::ddb_messages_json::ddb_upsert_serde_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test messages::ddb_messages_json::ddb_upsert_serde_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::ddb_signon_handler::test_on_ddb_signon_updates_backend_and_sends_response" => {
            match panic::catch_unwind(|| crate::messages::ddb_signon_handler::test_on_ddb_signon_updates_backend_and_sends_response()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::ddb_signon_handler::test_on_ddb_signon_updates_backend_and_sends_response panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::ddb_signon_handler::test_on_ddb_signon_updates_backend_and_sends_response panicked: {}", s); }
                    else { tracing::error!("Test messages::ddb_signon_handler::test_on_ddb_signon_updates_backend_and_sends_response panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::listening_notifications::triangle_test3_notifies_listening_true" => {
            match panic::catch_unwind(|| crate::messages::listening_notifications::triangle_test3_notifies_listening_true()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::listening_notifications::triangle_test3_notifies_listening_true panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::listening_notifications::triangle_test3_notifies_listening_true panicked: {}", s); }
                    else { tracing::error!("Test messages::listening_notifications::triangle_test3_notifies_listening_true panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::marshal_ping::unit_ping_ping_from_json" => {
            match panic::catch_unwind(|| crate::messages::marshal_ping::unit_ping_ping_from_json()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::marshal_ping::unit_ping_ping_from_json panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::marshal_ping::unit_ping_ping_from_json panicked: {}", s); }
                    else { tracing::error!("Test messages::marshal_ping::unit_ping_ping_from_json panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::marshal_ping::unit_ping_response_to_json" => {
            match panic::catch_unwind(|| crate::messages::marshal_ping::unit_ping_response_to_json()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::marshal_ping::unit_ping_response_to_json panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::marshal_ping::unit_ping_response_to_json panicked: {}", s); }
                    else { tracing::error!("Test messages::marshal_ping::unit_ping_response_to_json panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::marshal_relay_call::unit_serialize_relay_call_and_roundtrip" => {
            match panic::catch_unwind(|| crate::messages::marshal_relay_call::unit_serialize_relay_call_and_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_call_and_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_call_and_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_call_and_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::marshal_relay_call::unit_serialize_relay_call_response_and_roundtrip" => {
            match panic::catch_unwind(|| crate::messages::marshal_relay_call::unit_serialize_relay_call_response_and_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_call_response_and_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_call_response_and_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_call_response_and_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::marshal_relay_call::unit_serialize_relay_listen" => {
            match panic::catch_unwind(|| crate::messages::marshal_relay_call::unit_serialize_relay_listen()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_listen panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_listen panicked: {}", s); }
                    else { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_listen panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::marshal_relay_call::unit_serialize_relay_listen_response" => {
            match panic::catch_unwind(|| crate::messages::marshal_relay_call::unit_serialize_relay_listen_response()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_listen_response panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_listen_response panicked: {}", s); }
                    else { tracing::error!("Test messages::marshal_relay_call::unit_serialize_relay_listen_response panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::marshal_triangle_response::unit_triangle_test1_response_from_json" => {
            match panic::catch_unwind(|| crate::messages::marshal_triangle_response::unit_triangle_test1_response_from_json()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::marshal_triangle_response::unit_triangle_test1_response_from_json panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::marshal_triangle_response::unit_triangle_test1_response_from_json panicked: {}", s); }
                    else { tracing::error!("Test messages::marshal_triangle_response::unit_triangle_test1_response_from_json panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::marshal_unit::unit_plain_text_from_json" => {
            match panic::catch_unwind(|| crate::messages::marshal_unit::unit_plain_text_from_json()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::marshal_unit::unit_plain_text_from_json panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::marshal_unit::unit_plain_text_from_json panicked: {}", s); }
                    else { tracing::error!("Test messages::marshal_unit::unit_plain_text_from_json panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::marshal_unit::unit_triangle_test1_from_json" => {
            match panic::catch_unwind(|| crate::messages::marshal_unit::unit_triangle_test1_from_json()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::marshal_unit::unit_triangle_test1_from_json panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::marshal_unit::unit_triangle_test1_from_json panicked: {}", s); }
                    else { tracing::error!("Test messages::marshal_unit::unit_triangle_test1_from_json panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::mutex_messages_json::unit_mutex_release_roundtrip" => {
            match panic::catch_unwind(|| crate::messages::mutex_messages_json::unit_mutex_release_roundtrip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::mutex_messages_json::unit_mutex_release_roundtrip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::mutex_messages_json::unit_mutex_release_roundtrip panicked: {}", s); }
                    else { tracing::error!("Test messages::mutex_messages_json::unit_mutex_release_roundtrip panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::mutex_messages_json::unit_mutex_request_from_json" => {
            match panic::catch_unwind(|| crate::messages::mutex_messages_json::unit_mutex_request_from_json()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::mutex_messages_json::unit_mutex_request_from_json panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::mutex_messages_json::unit_mutex_request_from_json panicked: {}", s); }
                    else { tracing::error!("Test messages::mutex_messages_json::unit_mutex_request_from_json panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::mutex_messages_json::unit_mutex_response_to_json" => {
            match panic::catch_unwind(|| crate::messages::mutex_messages_json::unit_mutex_response_to_json()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::mutex_messages_json::unit_mutex_response_to_json panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::mutex_messages_json::unit_mutex_response_to_json panicked: {}", s); }
                    else { tracing::error!("Test messages::mutex_messages_json::unit_mutex_response_to_json panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::on_plain_text_delegate::on_plain_text_calls_handler_implementation" => {
            match panic::catch_unwind(|| crate::messages::on_plain_text_delegate::on_plain_text_calls_handler_implementation()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::on_plain_text_delegate::on_plain_text_calls_handler_implementation panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::on_plain_text_delegate::on_plain_text_calls_handler_implementation panicked: {}", s); }
                    else { tracing::error!("Test messages::on_plain_text_delegate::on_plain_text_calls_handler_implementation panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::ping_routing::route_invokes_on_ping_ping" => {
            match panic::catch_unwind(|| crate::messages::ping_routing::route_invokes_on_ping_ping()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::ping_routing::route_invokes_on_ping_ping panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::ping_routing::route_invokes_on_ping_ping panicked: {}", s); }
                    else { tracing::error!("Test messages::ping_routing::route_invokes_on_ping_ping panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::relay_call::relay_call_allocates_channel_and_maps_pair" => {
            match panic::catch_unwind(|| crate::messages::relay_call::relay_call_allocates_channel_and_maps_pair()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::relay_call::relay_call_allocates_channel_and_maps_pair panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::relay_call::relay_call_allocates_channel_and_maps_pair panicked: {}", s); }
                    else { tracing::error!("Test messages::relay_call::relay_call_allocates_channel_and_maps_pair panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::relay_called_handler::relay_called_handler_invokes_turn_handle_called" => {
            match panic::catch_unwind(|| crate::messages::relay_called_handler::relay_called_handler_invokes_turn_handle_called()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::relay_called_handler::relay_called_handler_invokes_turn_handle_called panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::relay_called_handler::relay_called_handler_invokes_turn_handle_called panicked: {}", s); }
                    else { tracing::error!("Test messages::relay_called_handler::relay_called_handler_invokes_turn_handle_called panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::relay_listen::relay_listen_registers_and_responds" => {
            match panic::catch_unwind(|| crate::messages::relay_listen::relay_listen_registers_and_responds()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::relay_listen::relay_listen_registers_and_responds panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::relay_listen::relay_listen_registers_and_responds panicked: {}", s); }
                    else { tracing::error!("Test messages::relay_listen::relay_listen_registers_and_responds panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::relay_ping_handler_unit::relay_ping_handler_uses_api_get_my_id_for_checking_id" => {
            match panic::catch_unwind(|| crate::messages::relay_ping_handler_unit::relay_ping_handler_uses_api_get_my_id_for_checking_id()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::relay_ping_handler_unit::relay_ping_handler_uses_api_get_my_id_for_checking_id panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::relay_ping_handler_unit::relay_ping_handler_uses_api_get_my_id_for_checking_id panicked: {}", s); }
                    else { tracing::error!("Test messages::relay_ping_handler_unit::relay_ping_handler_uses_api_get_my_id_for_checking_id panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::relay_ping_handlers::on_triangle_test1_sends_triangle_test2_to_peer" => {
            match panic::catch_unwind(|| crate::messages::relay_ping_handlers::on_triangle_test1_sends_triangle_test2_to_peer()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::relay_ping_handlers::on_triangle_test1_sends_triangle_test2_to_peer panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::relay_ping_handlers::on_triangle_test1_sends_triangle_test2_to_peer panicked: {}", s); }
                    else { tracing::error!("Test messages::relay_ping_handlers::on_triangle_test1_sends_triangle_test2_to_peer panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::relay_ping_handlers::on_triangle_test2_sends_triangle_test3_to_endpoint" => {
            match panic::catch_unwind(|| crate::messages::relay_ping_handlers::on_triangle_test2_sends_triangle_test3_to_endpoint()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::relay_ping_handlers::on_triangle_test2_sends_triangle_test3_to_endpoint panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::relay_ping_handlers::on_triangle_test2_sends_triangle_test3_to_endpoint panicked: {}", s); }
                    else { tracing::error!("Test messages::relay_ping_handlers::on_triangle_test2_sends_triangle_test3_to_endpoint panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::relay_triangle_test1_ext::test_relay_finder_honors_exclusions" => {
            match panic::catch_unwind(|| crate::messages::relay_triangle_test1_ext::test_relay_finder_honors_exclusions()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_finder_honors_exclusions panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_finder_honors_exclusions panicked: {}", s); }
                    else { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_finder_honors_exclusions panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::relay_triangle_test1_ext::test_relay_ping_handler_honors_exclusions" => {
            match panic::catch_unwind(|| crate::messages::relay_triangle_test1_ext::test_relay_ping_handler_honors_exclusions()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_ping_handler_honors_exclusions panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_ping_handler_honors_exclusions panicked: {}", s); }
                    else { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_ping_handler_honors_exclusions panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::relay_triangle_test1_ext::test_relay_triangle_test1_json_no_exclusions" => {
            match panic::catch_unwind(|| crate::messages::relay_triangle_test1_ext::test_relay_triangle_test1_json_no_exclusions()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_triangle_test1_json_no_exclusions panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_triangle_test1_json_no_exclusions panicked: {}", s); }
                    else { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_triangle_test1_json_no_exclusions panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::relay_triangle_test1_ext::test_relay_triangle_test1_json_with_exclusions" => {
            match panic::catch_unwind(|| crate::messages::relay_triangle_test1_ext::test_relay_triangle_test1_json_with_exclusions()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_triangle_test1_json_with_exclusions panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_triangle_test1_json_with_exclusions panicked: {}", s); }
                    else { tracing::error!("Test messages::relay_triangle_test1_ext::test_relay_triangle_test1_json_with_exclusions panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::router_from_id::route_passes_from_id_into_handler" => {
            match panic::catch_unwind(|| crate::messages::router_from_id::route_passes_from_id_into_handler()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::router_from_id::route_passes_from_id_into_handler panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::router_from_id::route_passes_from_id_into_handler panicked: {}", s); }
                    else { tracing::error!("Test messages::router_from_id::route_passes_from_id_into_handler panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::triangle_response_routing::router_dispatches_triangle_test1_response" => {
            match panic::catch_unwind(|| crate::messages::triangle_response_routing::router_dispatches_triangle_test1_response()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::triangle_response_routing::router_dispatches_triangle_test1_response panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::triangle_response_routing::router_dispatches_triangle_test1_response panicked: {}", s); }
                    else { tracing::error!("Test messages::triangle_response_routing::router_dispatches_triangle_test1_response panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::triangle_test1_response_sets_state::triangle_test1_response_does_not_override_endpoint_available" => {
            match panic::catch_unwind(|| crate::messages::triangle_test1_response_sets_state::triangle_test1_response_does_not_override_endpoint_available()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::triangle_test1_response_sets_state::triangle_test1_response_does_not_override_endpoint_available panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::triangle_test1_response_sets_state::triangle_test1_response_does_not_override_endpoint_available panicked: {}", s); }
                    else { tracing::error!("Test messages::triangle_test1_response_sets_state::triangle_test1_response_does_not_override_endpoint_available panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::triangle_test1_response_sets_state::triangle_test1_response_sets_nat_restricted_when_not_available" => {
            match panic::catch_unwind(|| crate::messages::triangle_test1_response_sets_state::triangle_test1_response_sets_nat_restricted_when_not_available()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::triangle_test1_response_sets_state::triangle_test1_response_sets_nat_restricted_when_not_available panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::triangle_test1_response_sets_state::triangle_test1_response_sets_nat_restricted_when_not_available panicked: {}", s); }
                    else { tracing::error!("Test messages::triangle_test1_response_sets_state::triangle_test1_response_sets_nat_restricted_when_not_available panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::triangle_test3_registers::triangle_test3_triggers_ddb_register_and_sets_registered" => {
            match panic::catch_unwind(|| crate::messages::triangle_test3_registers::triangle_test3_triggers_ddb_register_and_sets_registered()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::triangle_test3_registers::triangle_test3_triggers_ddb_register_and_sets_registered panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::triangle_test3_registers::triangle_test3_triggers_ddb_register_and_sets_registered panicked: {}", s); }
                    else { tracing::error!("Test messages::triangle_test3_registers::triangle_test3_triggers_ddb_register_and_sets_registered panicked with unknown error"); }
                    false
                }
            }
        },
        "messages::triangle_test3_registers::triangle_test3_triggers_relay_registration_sequence" => {
            match panic::catch_unwind(|| crate::messages::triangle_test3_registers::triangle_test3_triggers_relay_registration_sequence()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test messages::triangle_test3_registers::triangle_test3_triggers_relay_registration_sequence panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test messages::triangle_test3_registers::triangle_test3_triggers_relay_registration_sequence panicked: {}", s); }
                    else { tracing::error!("Test messages::triangle_test3_registers::triangle_test3_triggers_relay_registration_sequence panicked with unknown error"); }
                    false
                }
            }
        },
        "protocol::cert_verify_dump::peer_certificate_handler_generates_dump_and_verifies" => {
            match panic::catch_unwind(|| crate::protocol::cert_verify_dump::peer_certificate_handler_generates_dump_and_verifies()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test protocol::cert_verify_dump::peer_certificate_handler_generates_dump_and_verifies panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test protocol::cert_verify_dump::peer_certificate_handler_generates_dump_and_verifies panicked: {}", s); }
                    else { tracing::error!("Test protocol::cert_verify_dump::peer_certificate_handler_generates_dump_and_verifies panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::clear_state_cache::clear_state_cache_resets_and_reloads" => {
            match panic::catch_unwind(|| crate::relay::clear_state_cache::clear_state_cache_resets_and_reloads()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::clear_state_cache::clear_state_cache_resets_and_reloads panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::clear_state_cache::clear_state_cache_resets_and_reloads panicked: {}", s); }
                    else { tracing::error!("Test relay::clear_state_cache::clear_state_cache_resets_and_reloads panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::exclude_self_from_ddb::find_relay_does_not_select_self_even_if_ddb_includes_self" => {
            match panic::catch_unwind(|| crate::relay::exclude_self_from_ddb::find_relay_does_not_select_self_even_if_ddb_includes_self()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::exclude_self_from_ddb::find_relay_does_not_select_self_even_if_ddb_includes_self panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::exclude_self_from_ddb::find_relay_does_not_select_self_even_if_ddb_includes_self panicked: {}", s); }
                    else { tracing::error!("Test relay::exclude_self_from_ddb::find_relay_does_not_select_self_even_if_ddb_includes_self panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::exclude_self_from_ddb::list_all_relays_excludes_self_from_ddb" => {
            match panic::catch_unwind(|| crate::relay::exclude_self_from_ddb::list_all_relays_excludes_self_from_ddb()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::exclude_self_from_ddb::list_all_relays_excludes_self_from_ddb panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::exclude_self_from_ddb::list_all_relays_excludes_self_from_ddb panicked: {}", s); }
                    else { tracing::error!("Test relay::exclude_self_from_ddb::list_all_relays_excludes_self_from_ddb panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::list_all_relays_one_root::list_all_relays_queries_root_even_if_only_one" => {
            match panic::catch_unwind(|| crate::relay::list_all_relays_one_root::list_all_relays_queries_root_even_if_only_one()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::list_all_relays_one_root::list_all_relays_queries_root_even_if_only_one panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::list_all_relays_one_root::list_all_relays_queries_root_even_if_only_one panicked: {}", s); }
                    else { tracing::error!("Test relay::list_all_relays_one_root::list_all_relays_queries_root_even_if_only_one panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::lookup_root_id::lookup_known_root_returns_endpoint" => {
            match panic::catch_unwind(|| crate::relay::lookup_root_id::lookup_known_root_returns_endpoint()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::lookup_root_id::lookup_known_root_returns_endpoint panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::lookup_root_id::lookup_known_root_returns_endpoint panicked: {}", s); }
                    else { tracing::error!("Test relay::lookup_root_id::lookup_known_root_returns_endpoint panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::lookup_root_id::lookup_unknown_root_returns_none" => {
            match panic::catch_unwind(|| crate::relay::lookup_root_id::lookup_unknown_root_returns_none()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::lookup_root_id::lookup_unknown_root_returns_none panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::lookup_root_id::lookup_unknown_root_returns_none panicked: {}", s); }
                    else { tracing::error!("Test relay::lookup_root_id::lookup_unknown_root_returns_none panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::relay_client_unit::call_resolves_relay_address_via_ddb_when_missing" => {
            match panic::catch_unwind(|| crate::relay::relay_client_unit::call_resolves_relay_address_via_ddb_when_missing()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::relay_client_unit::call_resolves_relay_address_via_ddb_when_missing panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::relay_client_unit::call_resolves_relay_address_via_ddb_when_missing panicked: {}", s); }
                    else { tracing::error!("Test relay::relay_client_unit::call_resolves_relay_address_via_ddb_when_missing panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::relay_client_unit::call_with_address_present_returns_endpoint_with_channel" => {
            match panic::catch_unwind(|| crate::relay::relay_client_unit::call_with_address_present_returns_endpoint_with_channel()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::relay_client_unit::call_with_address_present_returns_endpoint_with_channel panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::relay_client_unit::call_with_address_present_returns_endpoint_with_channel panicked: {}", s); }
                    else { tracing::error!("Test relay::relay_client_unit::call_with_address_present_returns_endpoint_with_channel panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::relay_finder_unit::find_root_relay_rejects_self" => {
            match panic::catch_unwind(|| crate::relay::relay_finder_unit::find_root_relay_rejects_self()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::relay_finder_unit::find_root_relay_rejects_self panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::relay_finder_unit::find_root_relay_rejects_self panicked: {}", s); }
                    else { tracing::error!("Test relay::relay_finder_unit::find_root_relay_rejects_self panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::relay_finder_unit::select_indices_partitions_for_multiple_ids" => {
            match panic::catch_unwind(|| crate::relay::relay_finder_unit::select_indices_partitions_for_multiple_ids()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::relay_finder_unit::select_indices_partitions_for_multiple_ids panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::relay_finder_unit::select_indices_partitions_for_multiple_ids panicked: {}", s); }
                    else { tracing::error!("Test relay::relay_finder_unit::select_indices_partitions_for_multiple_ids panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::relay_states::load_and_summarize_states" => {
            match panic::catch_unwind(|| crate::relay::relay_states::load_and_summarize_states()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::relay_states::load_and_summarize_states panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::relay_states::load_and_summarize_states panicked: {}", s); }
                    else { tracing::error!("Test relay::relay_states::load_and_summarize_states panicked with unknown error"); }
                    false
                }
            }
        },
        "relay::relay_states_own::own_state_is_marked_and_not_checked" => {
            match panic::catch_unwind(|| crate::relay::relay_states_own::own_state_is_marked_and_not_checked()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test relay::relay_states_own::own_state_is_marked_and_not_checked panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test relay::relay_states_own::own_state_is_marked_and_not_checked panicked: {}", s); }
                    else { tracing::error!("Test relay::relay_states_own::own_state_is_marked_and_not_checked panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::endpoint_finder_impl_send_handler::impl_uses_send_packet_handler_instead_of_udp" => {
            match panic::catch_unwind(|| crate::stun::endpoint_finder_impl_send_handler::impl_uses_send_packet_handler_instead_of_udp()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::endpoint_finder_impl_send_handler::impl_uses_send_packet_handler_instead_of_udp panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::endpoint_finder_impl_send_handler::impl_uses_send_packet_handler_instead_of_udp panicked: {}", s); }
                    else { tracing::error!("Test stun::endpoint_finder_impl_send_handler::impl_uses_send_packet_handler_instead_of_udp panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::endpoint_finder_tests::after_two_responses_polls_resume_on_repeat_interval" => {
            match panic::catch_unwind(|| crate::stun::endpoint_finder_tests::after_two_responses_polls_resume_on_repeat_interval()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::endpoint_finder_tests::after_two_responses_polls_resume_on_repeat_interval panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::endpoint_finder_tests::after_two_responses_polls_resume_on_repeat_interval panicked: {}", s); }
                    else { tracing::error!("Test stun::endpoint_finder_tests::after_two_responses_polls_resume_on_repeat_interval panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::endpoint_finder_tests::error_after_three_intervals_with_less_than_two_responders" => {
            match panic::catch_unwind(|| crate::stun::endpoint_finder_tests::error_after_three_intervals_with_less_than_two_responders()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::endpoint_finder_tests::error_after_three_intervals_with_less_than_two_responders panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::endpoint_finder_tests::error_after_three_intervals_with_less_than_two_responders panicked: {}", s); }
                    else { tracing::error!("Test stun::endpoint_finder_tests::error_after_three_intervals_with_less_than_two_responders panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::endpoint_finder_tests::nonresponsive_server_removed_after_three_search_polls" => {
            match panic::catch_unwind(|| crate::stun::endpoint_finder_tests::nonresponsive_server_removed_after_three_search_polls()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::endpoint_finder_tests::nonresponsive_server_removed_after_three_search_polls panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::endpoint_finder_tests::nonresponsive_server_removed_after_three_search_polls panicked: {}", s); }
                    else { tracing::error!("Test stun::endpoint_finder_tests::nonresponsive_server_removed_after_three_search_polls panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::endpoint_finder_tests::single_response_triggers_single_and_callback_without_ip" => {
            match panic::catch_unwind(|| crate::stun::endpoint_finder_tests::single_response_triggers_single_and_callback_without_ip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::endpoint_finder_tests::single_response_triggers_single_and_callback_without_ip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::endpoint_finder_tests::single_response_triggers_single_and_callback_without_ip panicked: {}", s); }
                    else { tracing::error!("Test stun::endpoint_finder_tests::single_response_triggers_single_and_callback_without_ip panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::endpoint_finder_tests::state_transitions_consistent_and_inconsistent" => {
            match panic::catch_unwind(|| crate::stun::endpoint_finder_tests::state_transitions_consistent_and_inconsistent()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::endpoint_finder_tests::state_transitions_consistent_and_inconsistent panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::endpoint_finder_tests::state_transitions_consistent_and_inconsistent panicked: {}", s); }
                    else { tracing::error!("Test stun::endpoint_finder_tests::state_transitions_consistent_and_inconsistent panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::endpoint_finder_tests::stop_stops_promptly" => {
            match panic::catch_unwind(|| crate::stun::endpoint_finder_tests::stop_stops_promptly()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::endpoint_finder_tests::stop_stops_promptly panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::endpoint_finder_tests::stop_stops_promptly panicked: {}", s); }
                    else { tracing::error!("Test stun::endpoint_finder_tests::stop_stops_promptly panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::endpoint_finder_tests::two_consistent_responses_trigger_consistent_with_ip_in_callback" => {
            match panic::catch_unwind(|| crate::stun::endpoint_finder_tests::two_consistent_responses_trigger_consistent_with_ip_in_callback()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::endpoint_finder_tests::two_consistent_responses_trigger_consistent_with_ip_in_callback panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::endpoint_finder_tests::two_consistent_responses_trigger_consistent_with_ip_in_callback panicked: {}", s); }
                    else { tracing::error!("Test stun::endpoint_finder_tests::two_consistent_responses_trigger_consistent_with_ip_in_callback panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::endpoint_finder_tests::two_consistent_then_one_inconsistent_switches_to_inconsistent_and_callback_without_ip" => {
            match panic::catch_unwind(|| crate::stun::endpoint_finder_tests::two_consistent_then_one_inconsistent_switches_to_inconsistent_and_callback_without_ip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::endpoint_finder_tests::two_consistent_then_one_inconsistent_switches_to_inconsistent_and_callback_without_ip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::endpoint_finder_tests::two_consistent_then_one_inconsistent_switches_to_inconsistent_and_callback_without_ip panicked: {}", s); }
                    else { tracing::error!("Test stun::endpoint_finder_tests::two_consistent_then_one_inconsistent_switches_to_inconsistent_and_callback_without_ip panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::endpoint_finder_tests::two_inconsistent_responses_trigger_inconsistent_callback_without_ip" => {
            match panic::catch_unwind(|| crate::stun::endpoint_finder_tests::two_inconsistent_responses_trigger_inconsistent_callback_without_ip()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::endpoint_finder_tests::two_inconsistent_responses_trigger_inconsistent_callback_without_ip panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::endpoint_finder_tests::two_inconsistent_responses_trigger_inconsistent_callback_without_ip panicked: {}", s); }
                    else { tracing::error!("Test stun::endpoint_finder_tests::two_inconsistent_responses_trigger_inconsistent_callback_without_ip panicked with unknown error"); }
                    false
                }
            }
        },
        "stun::simple_server_consistent::simple_stun_two_servers_consistent" => {
            match panic::catch_unwind(|| crate::stun::simple_server_consistent::simple_stun_two_servers_consistent()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test stun::simple_server_consistent::simple_stun_two_servers_consistent panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test stun::simple_server_consistent::simple_stun_two_servers_consistent panicked: {}", s); }
                    else { tracing::error!("Test stun::simple_server_consistent::simple_stun_two_servers_consistent panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::client_handler_impl::client_call_response_registers_allowed_and_channel" => {
            match panic::catch_unwind(|| crate::turn::client_handler_impl::client_call_response_registers_allowed_and_channel()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::client_handler_impl::client_call_response_registers_allowed_and_channel panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::client_handler_impl::client_call_response_registers_allowed_and_channel panicked: {}", s); }
                    else { tracing::error!("Test turn::client_handler_impl::client_call_response_registers_allowed_and_channel panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::client_handler_impl::client_called_after_listen_response_registers_channel" => {
            match panic::catch_unwind(|| crate::turn::client_handler_impl::client_called_after_listen_response_registers_channel()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::client_handler_impl::client_called_after_listen_response_registers_channel panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::client_handler_impl::client_called_after_listen_response_registers_channel panicked: {}", s); }
                    else { tracing::error!("Test turn::client_handler_impl::client_called_after_listen_response_registers_channel panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::client_handler_impl::client_handle_listen_fails_with_error" => {
            match panic::catch_unwind(|| crate::turn::client_handler_impl::client_handle_listen_fails_with_error()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::client_handler_impl::client_handle_listen_fails_with_error panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::client_handler_impl::client_handle_listen_fails_with_error panicked: {}", s); }
                    else { tracing::error!("Test turn::client_handler_impl::client_handle_listen_fails_with_error panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::client_handler_impl::client_incoming_from_called_relay_returns_wrapped" => {
            match panic::catch_unwind(|| crate::turn::client_handler_impl::client_incoming_from_called_relay_returns_wrapped()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::client_handler_impl::client_incoming_from_called_relay_returns_wrapped panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::client_handler_impl::client_incoming_from_called_relay_returns_wrapped panicked: {}", s); }
                    else { tracing::error!("Test turn::client_handler_impl::client_incoming_from_called_relay_returns_wrapped panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::client_handler_impl::client_incoming_from_listener_relay_on_open_channel" => {
            match panic::catch_unwind(|| crate::turn::client_handler_impl::client_incoming_from_listener_relay_on_open_channel()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::client_handler_impl::client_incoming_from_listener_relay_on_open_channel panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::client_handler_impl::client_incoming_from_listener_relay_on_open_channel panicked: {}", s); }
                    else { tracing::error!("Test turn::client_handler_impl::client_incoming_from_listener_relay_on_open_channel panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::client_handler_impl::client_listen_response_registers_allowed_but_not_channel" => {
            match panic::catch_unwind(|| crate::turn::client_handler_impl::client_listen_response_registers_allowed_but_not_channel()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::client_handler_impl::client_listen_response_registers_allowed_but_not_channel panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::client_handler_impl::client_listen_response_registers_allowed_but_not_channel panicked: {}", s); }
                    else { tracing::error!("Test turn::client_handler_impl::client_listen_response_registers_allowed_but_not_channel panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::client_handler_impl::client_send_outgoing_wraps_and_fails_without_channel" => {
            match panic::catch_unwind(|| crate::turn::client_handler_impl::client_send_outgoing_wraps_and_fails_without_channel()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::client_handler_impl::client_send_outgoing_wraps_and_fails_without_channel panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::client_handler_impl::client_send_outgoing_wraps_and_fails_without_channel panicked: {}", s); }
                    else { tracing::error!("Test turn::client_handler_impl::client_send_outgoing_wraps_and_fails_without_channel panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::client_impl::unit_turn_client_handle_call_response_and_send" => {
            match panic::catch_unwind(|| crate::turn::client_impl::unit_turn_client_handle_call_response_and_send()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::client_impl::unit_turn_client_handle_call_response_and_send panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::client_impl::unit_turn_client_handle_call_response_and_send panicked: {}", s); }
                    else { tracing::error!("Test turn::client_impl::unit_turn_client_handle_call_response_and_send panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::client_impl::unit_turn_client_handle_called_and_send" => {
            match panic::catch_unwind(|| crate::turn::client_impl::unit_turn_client_handle_called_and_send()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::client_impl::unit_turn_client_handle_called_and_send panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::client_impl::unit_turn_client_handle_called_and_send panicked: {}", s); }
                    else { tracing::error!("Test turn::client_impl::unit_turn_client_handle_called_and_send panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::handle_listen_validation::unit_turn_incoming_accepted_after_listen" => {
            match panic::catch_unwind(|| crate::turn::handle_listen_validation::unit_turn_incoming_accepted_after_listen()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::handle_listen_validation::unit_turn_incoming_accepted_after_listen panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::handle_listen_validation::unit_turn_incoming_accepted_after_listen panicked: {}", s); }
                    else { tracing::error!("Test turn::handle_listen_validation::unit_turn_incoming_accepted_after_listen panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::handle_listen_validation::unit_turn_incoming_rejected_without_listen_and_call" => {
            match panic::catch_unwind(|| crate::turn::handle_listen_validation::unit_turn_incoming_rejected_without_listen_and_call()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::handle_listen_validation::unit_turn_incoming_rejected_without_listen_and_call panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::handle_listen_validation::unit_turn_incoming_rejected_without_listen_and_call panicked: {}", s); }
                    else { tracing::error!("Test turn::handle_listen_validation::unit_turn_incoming_rejected_without_listen_and_call panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::relay_impl::relay_handle_call_sets_mappings_and_incoming_both_directions" => {
            match panic::catch_unwind(|| crate::turn::relay_impl::relay_handle_call_sets_mappings_and_incoming_both_directions()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::relay_impl::relay_handle_call_sets_mappings_and_incoming_both_directions panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::relay_impl::relay_handle_call_sets_mappings_and_incoming_both_directions panicked: {}", s); }
                    else { tracing::error!("Test turn::relay_impl::relay_handle_call_sets_mappings_and_incoming_both_directions panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::relay_impl::relay_handle_listen_updates_allowed_entries" => {
            match panic::catch_unwind(|| crate::turn::relay_impl::relay_handle_listen_updates_allowed_entries()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::relay_impl::relay_handle_listen_updates_allowed_entries panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::relay_impl::relay_handle_listen_updates_allowed_entries panicked: {}", s); }
                    else { tracing::error!("Test turn::relay_impl::relay_handle_listen_updates_allowed_entries panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::unit_turn_handle_call_allocates_in_range_and_reuses" => {
            match panic::catch_unwind(|| crate::turn::unit_turn_handle_call_allocates_in_range_and_reuses()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::unit_turn_handle_call_allocates_in_range_and_reuses panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::unit_turn_handle_call_allocates_in_range_and_reuses panicked: {}", s); }
                    else { tracing::error!("Test turn::unit_turn_handle_call_allocates_in_range_and_reuses panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::unit_turn_incoming_invalid_packets_return_none" => {
            match panic::catch_unwind(|| crate::turn::unit_turn_incoming_invalid_packets_return_none()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::unit_turn_incoming_invalid_packets_return_none panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::unit_turn_incoming_invalid_packets_return_none panicked: {}", s); }
                    else { tracing::error!("Test turn::unit_turn_incoming_invalid_packets_return_none panicked with unknown error"); }
                    false
                }
            }
        },
        "turn::unit_turn_wraps_and_unwraps_channel_data_with_padding" => {
            match panic::catch_unwind(|| crate::turn::unit_turn_wraps_and_unwraps_channel_data_with_padding()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test turn::unit_turn_wraps_and_unwraps_channel_data_with_padding panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test turn::unit_turn_wraps_and_unwraps_channel_data_with_padding panicked: {}", s); }
                    else { tracing::error!("Test turn::unit_turn_wraps_and_unwraps_channel_data_with_padding panicked with unknown error"); }
                    false
                }
            }
        },
        "util::net_det::depth_small_examples" => {
            match panic::catch_unwind(|| crate::util::net_det::depth_small_examples()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test util::net_det::depth_small_examples panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test util::net_det::depth_small_examples panicked: {}", s); }
                    else { tracing::error!("Test util::net_det::depth_small_examples panicked with unknown error"); }
                    false
                }
            }
        },
        "util::net_det::fill_and_flood_small_graph" => {
            match panic::catch_unwind(|| crate::util::net_det::fill_and_flood_small_graph()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test util::net_det::fill_and_flood_small_graph panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test util::net_det::fill_and_flood_small_graph panicked: {}", s); }
                    else { tracing::error!("Test util::net_det::fill_and_flood_small_graph panicked with unknown error"); }
                    false
                }
            }
        },
        "util::net_det::fill_edge_case_n1" => {
            match panic::catch_unwind(|| crate::util::net_det::fill_edge_case_n1()) {
                Ok(_) => true,
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<&str>() { tracing::error!("Test util::net_det::fill_edge_case_n1 panicked: {}", s); }
                    else if let Some(s) = e.downcast_ref::<String>() { tracing::error!("Test util::net_det::fill_edge_case_n1 panicked: {}", s); }
                    else { tracing::error!("Test util::net_det::fill_edge_case_n1 panicked with unknown error"); }
                    false
                }
            }
        },

        _ => false,
    }
}
