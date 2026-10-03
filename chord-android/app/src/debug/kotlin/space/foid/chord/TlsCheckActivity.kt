package space.foid.chord

import android.app.Activity
import android.os.Bundle
import android.util.Log
import kotlinx.coroutines.runBlocking
import uniffi.chord_ffi.ChordClient
import kotlin.concurrent.thread

/**
 * Debug only. Checks the TLS verification of the native core on a device. It logs under the
 * tag `ChordTls` and finishes.
 *
 * adb shell am start -n space.foid.chord/.TlsCheckActivity \
 *   --es jid user@example.org --es password dummy [--es server starttls://host:5222] \
 *   [--es urls https://example.com/,https://expired.badssl.com/]
 *
 * With a wrong password a good certificate gives `AuthFailed`. A bad certificate gives
 * `TlsInvalid`. Each URL gets one HTTPS HEAD request through the same client as uploads.
 */
class TlsCheckActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        NativeInit.init(applicationContext)
        val jid = intent.getStringExtra("jid")
        val password = intent.getStringExtra("password") ?: "dummy-password"
        val server = intent.getStringExtra("server") ?: ""
        val urls = intent.getStringExtra("urls")?.split(",")?.filter { it.isNotBlank() }.orEmpty()
        val db = cacheDir.resolve("tlscheck-${System.nanoTime()}.db").absolutePath
        thread(name = "tls-check") {
            if (jid != null) {
                val result = runCatching {
                    ChordClient(db, jid).use { client ->
                        runBlocking { client.login(jid, password, server) }
                    }
                }
                Log.i(TAG, "xmpp jid=$jid server='$server' result=${result.fold({ "LoggedIn" }, { "${it::class.simpleName}: ${it.message}" })}")
            }
            for (url in urls) {
                Log.i(TAG, "https $url -> ${NativeInit.probeHttps(url)}")
            }
            Log.i(TAG, "done")
            runOnUiThread { finish() }
        }
    }

    private companion object {
        const val TAG = "ChordTls"
    }
}
