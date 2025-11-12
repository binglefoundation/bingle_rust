use std::fs;
use std::net::{SocketAddr, ToSocketAddrs};

use crate::api::bingle_api::StartOptions;
use crate::blockchain::algo_ops::AlgoProviderConfig;
use serde::Deserialize;

/// Parse a comma or whitespace separated list of socket addresses or hostnames with ports.
/// Accepts entries like "1.2.3.4:3478" or "stun.example.com:3478".
fn parse_stun_list(s: &str) -> Result<Vec<SocketAddr>, String> {
    let mut addrs = Vec::new();
    for part in s.split(|c: char| c == ',' || c.is_whitespace()) {
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
                let (net, cfg) = parse_node_file(&v)?;
                algo_network = net;
                algo_provider_config = Some(cfg);
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

    Ok(StartOptions {
        handle,
        algo_passphrase,
        static_ip,
        am_relay,
        stun_servers,
        algo_provider_config,
        algo_network,
    })
}

#[cfg(test)]
mod tests_do_not_use_inline {
    // Intentionally left empty per project guideline: tests live under tests/.
}
