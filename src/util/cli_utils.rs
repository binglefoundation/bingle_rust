use crate::api::bingle_api::StartOptions;
use crate::blockchain::algo_ops::AlgoChainConfig;
use crate::util::config_utils;
use crate::util::logging::LogMode;
use std::net::SocketAddr;

/// Parse CLI arguments into StartOptions.
///
/// Supported options:
///  --handle <handle>
///  <positional_handle> (if --handle not given)
///  --passphrase <text>
///  --relay
///  --static-ip <ip:port>
///  --stun-servers <list>
///  --stun-servers-file <file>
///  --node-file <file>
pub fn parse_start_options_from_args<I>(args: I) -> Result<StartOptions, String>
where
    I: IntoIterator<Item = String>,
{
    let mut it = args.into_iter();
    let mut handle: Option<String> = None;
    let mut algo_passphrase: Option<String> = None;
    let mut am_relay = false;
    let mut static_ip: Option<SocketAddr> = None;
    let mut stun_servers: Option<Vec<SocketAddr>> = None;
    let mut algo_provider_config: Option<AlgoChainConfig> = None;
    let mut algo_network: Option<String> = None;
    let mut cli_app_id: Option<u64> = None;
    let mut cli_asset_id: Option<u64> = None;
    let mut node_app_id: Option<u64> = None;
    let mut node_asset_id: Option<u64> = None;
    let mut log_level: Option<String> = None;
    let mut log_mode = LogMode::Plain;
    let mut handle_cache_expiry: Option<std::time::Duration> = None;
    let dangerous_debug = false;

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--handle" => {
                let h = it.next().ok_or("--handle requires a value")?;
                handle = Some(h);
            }
            "--passphrase" => {
                let p = it.next().ok_or("--passphrase requires a value")?;
                algo_passphrase = Some(p);
            }
            "--relay" => {
                am_relay = true;
            }
            "--static-ip" => {
                let v = it.next().ok_or("--static-ip requires an <ip:port> value")?;
                let addr: SocketAddr = v
                    .parse()
                    .map_err(|e| format!("Invalid --static-ip '{}': {}", v, e))?;
                static_ip = Some(addr);
            }
            "--stun-servers" => {
                let v = it.next().ok_or("--stun-servers requires a value")?;
                let list = config_utils::parse_stun_list(&v)?;
                stun_servers = Some(list);
            }
            "--stun-servers-file" => {
                let v = it
                    .next()
                    .ok_or("--stun-servers-file requires a <file> value")?;
                let list = config_utils::parse_stun_file(&v)?;
                stun_servers = Some(list);
            }
            "--node-file" => {
                let v = it.next().ok_or("--node-file requires a <file> value")?;
                let (net, cfg, nid_app, nid_asset) = config_utils::parse_node_file_with_ids(&v)?;
                algo_network = net;
                algo_provider_config = Some(cfg);
                node_app_id = nid_app;
                node_asset_id = nid_asset;
            }
            "--log-level" => {
                let v = it
                    .next()
                    .ok_or("--log-level requires a value (trace|debug|info|warn|error)")?;
                log_level = Some(v);
            }
            "--log-mode" => {
                let v = it
                    .next()
                    .ok_or("--log-mode requires a value (Plain|ANSI|AWS|JS)")?;
                log_mode = match v.to_ascii_lowercase().as_str() {
                    "plain" => LogMode::Plain,
                    "ansi" => LogMode::ANSI,
                    "aws" => LogMode::AWS,
                    "js" => LogMode::JS,
                    _ => {
                        return Err(format!(
                            "Invalid --log-mode '{}': must be Plain|ANSI|AWS|JS",
                            v
                        ));
                    }
                };
            }
            "--app-id" => {
                let v = it.next().ok_or("--app-id requires a value")?;
                cli_app_id = Some(
                    v.parse::<u64>()
                        .map_err(|e| format!("Invalid --app-id '{}': {}", v, e))?,
                );
            }
            "--asset-id" => {
                let v = it.next().ok_or("--asset-id requires a value")?;
                cli_asset_id = Some(
                    v.parse::<u64>()
                        .map_err(|e| format!("Invalid --asset-id '{}': {}", v, e))?,
                );
            }
            "--handle-cache-expiry-secs" => {
                let v = it
                    .next()
                    .ok_or("--handle-cache-expiry-secs requires a <seconds> value")?;
                let secs = v
                    .parse::<u64>()
                    .map_err(|e| format!("Invalid --handle-cache-expiry-secs '{}': {}", v, e))?;
                handle_cache_expiry = Some(std::time::Duration::from_secs(secs));
            }
            "--debug" => {
                // Accept a --debug flag. The binary may use this to enable verbose output.
                // Intentionally no-op here to keep StartOptions stable for existing tests.
            }
            s if s.starts_with('-') => {
                return Err(format!("Unknown option: {}", s));
            }
            // positional
            other => {
                if handle.is_none() {
                    handle = Some(other.to_string());
                } else {
                    return Err(format!("Unexpected positional argument: {}", other));
                }
            }
        }
    }

    let handle =
        handle.ok_or("Missing handle: provide --handle <handle> or a positional <handle>")?;

    // Try to resolve IDs; if none provided anywhere, leave as None to keep start flexible for tests.
    let (app_id_opt, asset_id_opt) = match config_utils::resolve_app_asset_ids(
        node_app_id,
        node_asset_id,
        cli_app_id,
        cli_asset_id,
    ) {
        Ok((a, b)) => (Some(a), Some(b)),
        Err(_) => (None, None),
    };

    Ok(StartOptions {
        handle,
        algo_passphrase,
        static_ip,
        am_relay,
        stun_servers,
        algo_provider_config,
        algo_network,
        app_id: app_id_opt,
        asset_id: asset_id_opt,
        log_level,
        handle_cache_expiry,
        dangerous_debug,
        log_mode,
        wait_response_timeout: None,
    })
}
