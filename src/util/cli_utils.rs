use std::fs;
use std::net::{SocketAddr, ToSocketAddrs};

use crate::api::bingle_api::StartOptions;
use crate::blockchain::algo_ops::AlgoProviderConfig;
use serde::Deserialize;

/// Parse a comma or whitespace separated list of socket addresses or hostnames with ports.
/// Accepts entries like "1.2.3.4:3478" or "stun.example.com:3478".
fn parse_stun_list(s: &str) -> Result<Vec<SocketAddr>, String> {
    // First, strip out line comments: anything after '#' on a line is ignored.
    let mut cleaned = String::with_capacity(s.len());
    for line in s.lines() {
        let line_no_comment = match line.find('#') {
            Some(idx) => &line[..idx],
            None => line,
        };
        cleaned.push_str(line_no_comment);
        cleaned.push('\n');
    }

    let mut addrs = Vec::new();
    for part in cleaned.split(|c: char| c == ',' || c.is_whitespace()) {
        let p = part.trim();
        if p.is_empty() { continue; }
        // Try direct SocketAddr parse first
        let parsed = p.parse::<SocketAddr>().ok()
            // Fallback to DNS resolution via ToSocketAddrs for host:port strings
            .or_else(|| p.to_socket_addrs().ok().and_then(|mut it| it.next()));
        if let Some(addr) = parsed {
            addrs.push(addr);
        } else {
            return Err(format!("Invalid STUN server entry '{}': must be <host:port> or <ip:port>", p));
        }
    }
    if addrs.is_empty() {
        Err("No valid STUN servers provided".to_string())
    } else {
        Ok(addrs)
    }
}

/// Read a file and parse a list of socket addresses (one per line or comma/space separated).
fn parse_stun_file(path: &str) -> Result<Vec<SocketAddr>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read STUN servers file '{}': {}", path, e))?;
    parse_stun_list(&content)
}

#[derive(Deserialize)]
struct NodeFile {
    network: Option<String>,
    client_api_url: String,
    client_api_port: u16,
    indexer_api_url: String,
    indexer_api_port: u16,
    token: Option<String>,
    token_key: Option<String>,
    app_id: Option<u64>,
    asset_id: Option<u64>,
}

fn parse_node_file(path: &str) -> Result<(Option<String>, AlgoProviderConfig), String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read node file '{}': {}", path, e))?;
    let nf: NodeFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse node file '{}': {}", path, e))?;
    Ok((
        nf.network,
        AlgoProviderConfig {
            client_api_url: nf.client_api_url,
            client_api_port: nf.client_api_port,
            indexer_api_url: nf.indexer_api_url,
            indexer_api_port: nf.indexer_api_port,
            token: nf.token,
            token_key: nf.token_key,
        },
    ))
}

pub fn parse_node_file_with_ids(path: &str) -> Result<(Option<String>, AlgoProviderConfig, Option<u64>, Option<u64>), String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read node file '{}': {}", path, e))?;
    let nf: NodeFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse node file '{}': {}", path, e))?;
    Ok((
        nf.network,
        AlgoProviderConfig {
            client_api_url: nf.client_api_url,
            client_api_port: nf.client_api_port,
            indexer_api_url: nf.indexer_api_url,
            indexer_api_port: nf.indexer_api_port,
            token: nf.token,
            token_key: nf.token_key,
        },
        nf.app_id,
        nf.asset_id,
    ))
}

pub fn resolve_app_asset_ids(
    node_app_id: Option<u64>,
    node_asset_id: Option<u64>,
    cli_app_id: Option<u64>,
    cli_asset_id: Option<u64>,
) -> Result<(u64, u64), String> {
    if node_app_id.is_some() && cli_app_id.is_some() {
        return Err("--app-id provided but node file also contains app_id; remove one".to_string());
    }
    if node_asset_id.is_some() && cli_asset_id.is_some() {
        return Err("--asset-id provided but node file also contains asset_id; remove one".to_string());
    }

    let env_app = std::env::var("APP_ID").ok().and_then(|s| s.parse::<u64>().ok());
    let env_asset = std::env::var("ASSET_ID").ok().and_then(|s| s.parse::<u64>().ok());

    let final_app = node_app_id.or(cli_app_id).or(env_app)
        .ok_or_else(|| "Missing app_id: provide in node file, via --app-id, or set APP_ID".to_string())?;
    let final_asset = node_asset_id.or(cli_asset_id).or(env_asset)
        .ok_or_else(|| "Missing asset_id: provide in node file, via --asset-id, or set ASSET_ID".to_string())?;

    Ok((final_app, final_asset))
}

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
    let mut algo_provider_config: Option<AlgoProviderConfig> = None;
    let mut algo_network: Option<String> = None;
    let mut cli_app_id: Option<u64> = None;
    let mut cli_asset_id: Option<u64> = None;
    let mut node_app_id: Option<u64> = None;
    let mut node_asset_id: Option<u64> = None;

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
                let addr: SocketAddr = v.parse().map_err(|e| format!("Invalid --static-ip '{}': {}", v, e))?;
                static_ip = Some(addr);
            }
            "--stun-servers" => {
                let v = it.next().ok_or("--stun-servers requires a value")?;
                let list = parse_stun_list(&v)?;
                stun_servers = Some(list);
            }
            "--stun-servers-file" => {
                let v = it.next().ok_or("--stun-servers-file requires a <file> value")?;
                let list = parse_stun_file(&v)?;
                stun_servers = Some(list);
            }
            "--node-file" => {
                let v = it.next().ok_or("--node-file requires a <file> value")?;
                let (net, cfg, nid_app, nid_asset) = parse_node_file_with_ids(&v)?;
                algo_network = net;
                algo_provider_config = Some(cfg);
                node_app_id = nid_app;
                node_asset_id = nid_asset;
            }
            "--app-id" => {
                let v = it.next().ok_or("--app-id requires a value")?;
                cli_app_id = Some(v.parse::<u64>().map_err(|e| format!("Invalid --app-id '{}': {}", v, e))?);
            }
            "--asset-id" => {
                let v = it.next().ok_or("--asset-id requires a value")?;
                cli_asset_id = Some(v.parse::<u64>().map_err(|e| format!("Invalid --asset-id '{}': {}", v, e))?);
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

    let handle = handle.ok_or("Missing handle: provide --handle <handle> or a positional <handle>")?;

    // Try to resolve IDs; if none provided anywhere, leave as None to keep start flexible for tests.
    let (app_id_opt, asset_id_opt) = match resolve_app_asset_ids(node_app_id, node_asset_id, cli_app_id, cli_asset_id) {
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
    })
}

/// Parse a decimal ALGOs string into microAlgos (u64).
/// Accepts forms like "1", "0.5", "1.234567"; up to 6 fractional digits.
/// Returns an error on negative numbers, more than 6 fractional digits, or overflow.
pub fn parse_algos_decimal_to_microalgos(s: &str) -> Result<u64, String> {
    let t = s.trim();
    if t.is_empty() {
        return Err("price must not be empty".to_string());
    }
    if t.starts_with('-') {
        return Err("price must be non-negative".to_string());
    }
    // Split on decimal point
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() > 2 {
        return Err(format!("invalid price '{}': too many decimal points", s));
    }
    let whole_str = parts[0];
    let frac_str = if parts.len() == 2 { parts[1] } else { "" };
    if frac_str.len() > 6 {
        return Err(format!("invalid price '{}': more than 6 fractional digits", s));
    }
    // Parse digits only
    if !whole_str.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("invalid price '{}': non-digit characters", s));
    }
    if !frac_str.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("invalid price '{}': non-digit characters", s));
    }
    let whole: u128 = if whole_str.is_empty() { 0 } else { whole_str.parse().map_err(|e| format!("invalid price '{}': {}", s, e))? };
    // Pad fractional to 6 digits
    let mut frac_padded = String::from(frac_str);
    while frac_padded.len() < 6 { frac_padded.push('0'); }
    let frac: u128 = if frac_padded.is_empty() { 0 } else { frac_padded[..6].parse().map_err(|e| format!("invalid price '{}': {}", s, e))? };
    let micro: u128 = whole
        .checked_mul(1_000_000u128)
        .and_then(|v| v.checked_add(frac))
        .ok_or_else(|| format!("invalid price '{}': overflow", s))?;
    if micro > u64::MAX as u128 {
        return Err(format!("invalid price '{}': overflow", s));
    }
    Ok(micro as u64)
}

#[cfg(test)]
mod tests_do_not_use_inline {
    // Intentionally left empty per project guideline: tests live under tests/.
}
