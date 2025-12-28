// Aggregator for DDB tests

#[path = "ddb/advert_record_json.rs"]
mod advert_record_json;

#[path = "ddb/backend.rs"]
mod backend;

#[path = "ddb/client/register_ip.rs"]
mod ddb_client_register_ip;

#[path = "ddb/client/lookup.rs"]
mod ddb_client_lookup;
