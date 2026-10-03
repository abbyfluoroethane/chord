package space.foid.chord.data

import android.util.Log

/**
 * Logs a warning. In a JVM unit test android.util.Log is a stub that throws, so this
 * falls back to stderr.
 */
internal fun logWarn(tag: String, message: String, error: Throwable? = null) {
    try {
        if (error != null) Log.w(tag, message, error) else Log.w(tag, message)
    } catch (_: Throwable) {
        System.err.println("W/$tag: $message${error?.let { " ($it)" } ?: ""}")
    }
}
