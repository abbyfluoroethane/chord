package space.foid.chord

import android.os.Process
import android.os.SystemClock
import android.util.Log

/** Start-up timing marks for logcat (`adb logcat -s ChordStartup`). Milliseconds since the process started. */
object Startup {
    fun mark(name: String) {
        try {
            Log.i("ChordStartup", "$name +${SystemClock.elapsedRealtime() - Process.getStartElapsedRealtime()}ms")
        } catch (_: Throwable) {
            // A JVM unit test has no android.util.Log.
        }
    }
}
