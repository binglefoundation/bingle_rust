package com.bingle.jsi

import android.content.Context
import android.util.Log

/**
 * Android TLS bootstrap (issue #135).
 *
 * Algorand HTTPS goes through rustls, whose certificate verification uses rustls-platform-verifier;
 * on Android that verifier must be initialized with the app [Context] before the first HTTPS request
 * or it panics. The native module is otherwise loaded via JNA (uniffi), which does not run
 * `JNI_OnLoad`, so we force JNI symbol resolution with `System.loadLibrary` and call an exported
 * native init method. Initialization is idempotent (the Rust side is backed by a `OnceCell`).
 */
object BingleJsiTls {
    @Volatile
    private var initialized = false

    init {
        // JNA (uniffi) already loads libbingle_jsi.so, but System.loadLibrary makes the JVM resolve
        // the exported nativeInitTls JNI symbol below.
        System.loadLibrary("bingle_jsi")
    }

    /** Native impl in src/android_tls.rs; forwards to rustls_platform_verifier::android. */
    external fun nativeInitTls(context: Context): Boolean

    /** Initialize the platform TLS verifier once. Safe to call repeatedly. */
    @Synchronized
    fun ensureInitialized(context: Context) {
        if (initialized) return
        try {
            nativeInitTls(context.applicationContext)
            initialized = true
        } catch (t: Throwable) {
            // Non-fatal here; a later HTTPS call would surface the underlying problem.
            Log.e("BingleJsiTls", "rustls-platform-verifier init failed", t)
        }
    }
}
