// Aggregator for DDB tests

#[path = "ddb/advert_record_json.rs"]
pub mod advert_record_json;

#[path = "ddb/backend.rs"]
pub mod backend;

#[path = "ddb/client/register_ip.rs"]
pub mod ddb_client_register_ip;

#[path = "ddb/client/lookup.rs"]
pub mod ddb_client_lookup;

#[path = "ddb/client/register_relay.rs"]
pub mod ddb_client_register_relay;

#[path = "ddb/advert_record_signing.rs"]
pub mod advert_record_signing;

#[path = "ddb/mandatory_verification.rs"]
pub mod mandatory_verification;
