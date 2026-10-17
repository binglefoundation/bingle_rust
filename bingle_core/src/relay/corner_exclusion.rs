//! Which relays a triangle test must not use as its corner node (issue #347).
//!
//! The triangle test only proves a full cone NAT if the corner node's `TriangleTest3` reaches the
//! client over a path the client never opened. The client lists every endpoint it has sent to in
//! `do_not_use_endpoints`; a relay choosing the corner skips any candidate that one of those sends
//! could have opened a path for.

use std::net::SocketAddr;

/// True if a corner candidate at `candidate` must be skipped because the client has sent to an
/// endpoint in `exclude`.
///
/// Matches on the IP alone: an address-restricted NAT admits packets from any port of an IP the
/// client has sent to, so a candidate sharing that IP on another port would fake a full cone.
/// Loopback is the exception: there is no NAT between loopback endpoints and the port is what
/// tells nodes apart (every localnet node shares 127.0.0.1), so both being loopback compares the
/// full address.
pub fn is_excluded_corner(candidate: SocketAddr, exclude: &[SocketAddr]) -> bool {
    exclude.iter().any(|seen| {
        if seen.ip().is_loopback() && candidate.ip().is_loopback() {
            *seen == candidate
        } else {
            seen.ip() == candidate.ip()
        }
    })
}
