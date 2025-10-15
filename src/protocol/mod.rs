/// Bingle protocol-wide constants

/// Global issuer suffix for Bingle identities used in certificate CNs and issuers.
pub const ISSUER_SUFFIX: &str = ".ids.bingle.home.arpa";
/// Virtual CA Common Name used for all Bingle-issued CA certificates.
pub const VIRTUAL_CA: &str = "virtual.bingle.home.arpa";

pub mod cert_verify;
