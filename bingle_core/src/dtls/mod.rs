pub mod dtls_debug;
pub mod dtls_openssl;
pub mod dtls_trait;
pub mod network_mux_trait;
pub mod network_mux_udp;

pub use dtls_openssl::openssl_impl::DtlsOpenSsl;
pub use dtls_trait::Dtls;
#[cfg(feature = "test-hooks")]
pub use dtls_trait::{HandleMessage, HandlePeerCertificate, Result};
pub use network_mux_trait::NetworkMux;
#[cfg(feature = "test-hooks")]
pub use network_mux_trait::{HandleDtls, HandleStun, HandleTurn};
pub use network_mux_udp::UdpNetworkMux;
#[cfg(feature = "test-hooks")]
pub use network_mux_udp::{MuxType, mux_type_for};
