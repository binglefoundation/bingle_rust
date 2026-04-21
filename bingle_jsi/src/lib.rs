uniffi::setup_scaffolding!();

pub mod api;

use std::sync::Arc;

use api::bingle_jsi_api::BingleJsiApi;
use api::bingle_jsi_api_impl::BingleJsiApiImpl;
use api::error::BingleJsiError;
use api::types::BingleJsiConfig;

/// Create and initialize the Bingle JSI API from a typed configuration object.
///
/// This is the main entry point for React Native / TypeScript consumers.
/// The function name avoids Swift's reserved `init` keyword so that
/// uniffi-bindgen can generate valid Swift bindings.
#[uniffi::export]
pub fn create_bingle_api(config: BingleJsiConfig) -> Result<Arc<dyn BingleJsiApi>, BingleJsiError> {
    let impl_arc = BingleJsiApiImpl::init(config)?;
    Ok(impl_arc as Arc<dyn BingleJsiApi>)
}
