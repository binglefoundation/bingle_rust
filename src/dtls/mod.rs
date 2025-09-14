pub mod dtls_trait;
pub mod dtls_debug;
#[cfg(not(target_os = "ios"))]
pub mod dtls_openssl;
pub mod network_mux_trait;
pub mod network_mux_udp;

pub use dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result};
pub use network_mux_trait::{NetworkMux, HandleDtls, HandleStun, HandleTurn};
pub use network_mux_udp::{UdpNetworkMux, MuxType, mux_type_for};
#[cfg(not(target_os = "ios"))]
pub use dtls_openssl::non_ios::DtlsOpenSsl;
