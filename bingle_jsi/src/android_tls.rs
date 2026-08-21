//! Android TLS initialization (issue #135).
//!
//! Algorand HTTP goes through `algonaut` → `reqwest` (rustls), whose certificate verification uses
//! `rustls-platform-verifier`. On Android that verifier must be initialized with the JNI `JavaVM` +
//! an Android `Context` before the first HTTPS request, or it panics
//! ("Expect rustls-platform-verifier to be initialized"). iOS needs no such step.
//!
//! `bingle_jsi` loads its `.so` via JNA (uniffi), and JNA does not invoke `JNI_OnLoad`, so the usual
//! hook does not fire. Instead we export a native method that the Kotlin bridge calls once at
//! startup, passing the application `Context` (see `BingleJsiTls.kt`); it forwards to
//! `rustls_platform_verifier::android::init_with_env`, which stores the handles globally for the
//! reqwest verifier to use. Initialization is idempotent (backed by a `OnceCell`).

use jni::EnvUnowned;
use jni::objects::JObject;
use jni::sys::jboolean;

/// Native impl of `com.bingle.jsi.BingleJsiTls.nativeInitTls(Context): Boolean`. Exported with the
/// JNI-mangled name so `System.loadLibrary("bingle_jsi")` + a Kotlin `external fun` resolves it.
#[unsafe(export_name = "Java_com_bingle_jsi_BingleJsiTls_nativeInitTls")]
extern "system" fn native_init_tls<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    context: JObject<'local>,
) -> jboolean {
    env.with_env(|env| {
        rustls_platform_verifier::android::init_with_env(env, context)?;
        // jni-sys maps jboolean -> Rust bool.
        Ok::<jboolean, jni::errors::Error>(true)
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}
