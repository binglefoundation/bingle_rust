/// Outcome of the pre-start check that the configured app is still supported.
/// Mirrors the AppSupport enum in bingle_cli.rs for testability.
#[derive(Debug, PartialEq, Eq)]
enum AppSupport {
    NoAppId,
    Supported,
    Superseded { app_id: u64, successor: u64 },
}

/// Mirrors resolve_app_support from bingle_cli.rs.
fn resolve_app_support(app_id: Option<u64>, successor_app: Option<u64>) -> AppSupport {
    match app_id {
        None => AppSupport::NoAppId,
        Some(app_id) => match successor_app {
            Some(successor) => AppSupport::Superseded { app_id, successor },
            None => AppSupport::Supported,
        },
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_app_id_returns_no_app_id() {
    // No configured app_id: nothing to check, regardless of the successor value.
    assert_eq!(resolve_app_support(None, None), AppSupport::NoAppId);
    assert_eq!(resolve_app_support(None, Some(999)), AppSupport::NoAppId);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn app_id_without_successor_is_supported() {
    assert_eq!(
        resolve_app_support(Some(12345), None),
        AppSupport::Supported
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn app_id_with_successor_is_superseded() {
    assert_eq!(
        resolve_app_support(Some(100), Some(200)),
        AppSupport::Superseded {
            app_id: 100,
            successor: 200,
        }
    );
}
