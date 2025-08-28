pub mod dtls_trait;
pub mod dtls_udp_dtls;
#[cfg(not(target_os = "ios"))]
pub mod dtls_openssl;

pub use dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result};
pub use dtls_udp_dtls::DtlsUdpDtls;
#[cfg(not(target_os = "ios"))]
pub use dtls_openssl::non_ios::DtlsOpenSsl;
