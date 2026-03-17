use std::sync::Arc;
use rust_comms::api::bingle_api::{BingleApiBoth, BingleApiBothType};

pub fn to_weak<T: BingleApiBoth + 'static>(api: T) -> BingleApiBothType {
    let arc: Arc<dyn BingleApiBoth> = Arc::new(api);
    let weak = Arc::downgrade(&arc);
    Box::leak(Box::new(arc));
    weak
}

pub fn mock_api_weak() -> BingleApiBothType {
    use crate::util::reusable_mock_api::MockApiBoth;
    to_weak(MockApiBoth::new())
}
