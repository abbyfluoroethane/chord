package space.foid.chord

import android.content.Context

/**
 * Native set-up that UniFFI cannot do. Call `NativeInit.init(applicationContext)` once in
 * `Application.onCreate`, before any login or upload. It gives the JVM and the application
 * context to the Rust certificate verifier of HTTPS uploads. Without it the first upload
 * panics. See docs/android-tls.md.
 */
object NativeInit {
    init {
        System.loadLibrary("chord_ffi")
    }

    /** Safe to call more than once. Pass the application context. */
    @JvmStatic
    external fun init(context: Context)

    /** Debug diagnostic: one HTTPS HEAD request. Returns "ok <status>" or "error <text>". Blocks. */
    @JvmStatic
    external fun probeHttps(url: String): String
}
