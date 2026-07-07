use bingle_core::util::printing::enable_immediate_prints;

// Simple smoke test to ensure the portability shims in util::printing link and run.
// The function is idempotent and should not panic on any supported platform.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn enable_immediate_prints_is_idempotent_and_safe() {
    enable_immediate_prints();
    // Call twice to verify Once gating works and no double-initialization issues occur.
    enable_immediate_prints();
    // If we reach here without panic, test passes.
}
