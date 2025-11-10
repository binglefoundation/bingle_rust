use std::fs;
use std::net::SocketAddr;

use crate::api::bingle_api::StartOptions;

/// Parse a comma or whitespace separated list of socket addresses.
fn parse_stun_list(s: &str) -> Result<Vec<SocketAddr>, String> {
    let mut addrs = Vec::new();
    for part in s.split(|c: char| c == ',' || c.is_whitespace()) {
        let p = part.trim();
        if p.is_empty() { continue; }
        let addr: SocketAddr = p.parse().map_err(|e| format!("Invalid STUN server address '{}': {}", p, e))?;
        addrs.push(addr);
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
    })
}

#[cfg(test)]
mod tests_do_not_use_inline {
    // Intentionally left empty per project guideline: tests live under tests/.
}
