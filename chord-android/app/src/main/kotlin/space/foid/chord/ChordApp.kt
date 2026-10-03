package space.foid.chord

import android.app.Application
import space.foid.chord.data.ChordSession
import space.foid.chord.data.ChordSessionImpl
import space.foid.chord.notify.ChordNotifications
import space.foid.chord.secure.KeystoreCredentialStore
import space.foid.chord.service.ServiceControlImpl
import java.io.File

/** The application. It holds the one [ChordSession] of the process (manual DI, no Hilt). */
class ChordApp : Application() {
    /** The signed-in account. Created on first use. */
    val session: ChordSession by lazy {
        ChordSessionImpl(
            accountsDir = File(filesDir, "accounts"),
            credentials = KeystoreCredentialStore(this),
            service = ServiceControlImpl(this),
        )
    }

    override fun onCreate() {
        super.onCreate()
        ChordNotifications.createChannels(this)
    }
}
