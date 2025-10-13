use std::sync::Once;

static INIT: Once = Once::new();

/// Configure the process stdio to flush immediately so println!/eprintln! appear right away.
///
/// This is intended for tests and local runs where immediate log visibility helps debugging.
/// It is safe to call multiple times; the configuration is applied only once.
pub fn enable_immediate_prints() {
    INIT.call_once(|| {
        // Best-effort: on Unix-like systems use libc::setvbuf to disable buffering on stdout/stderr.
        #[cfg(unix)]
        unsafe {
            // Safety: setvbuf is process-global and intended to be called early; we call it once.
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            {
                unsafe extern "C" {
                    static mut __stdoutp: *mut libc::FILE;
                    static mut __stderrp: *mut libc::FILE;
                }
                let _ = libc::setvbuf(__stdoutp, std::ptr::null_mut(), libc::_IONBF, 0);
                let _ = libc::setvbuf(__stderrp, std::ptr::null_mut(), libc::_IONBF, 0);
            }
            #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
            {
                let _ = libc::setvbuf(libc::stdout, std::ptr::null_mut(), libc::_IONBF, 0);
                let _ = libc::setvbuf(libc::stderr, std::ptr::null_mut(), libc::_IONBF, 0);
            }
        }
        // On non-Unix targets, there is no portable way to change buffering for Rust stdio.
        // As a fallback, perform an explicit flush once. Callers should prefer eprintln! which
        // is generally unbuffered by terminals and more promptly visible in many environments.
        #[cfg(not(unix))]
        {
            use std::io::Write;
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();
        }
    });
}