package space.foid.chord

import android.app.Application
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import space.foid.chord.data.ChordSession
import space.foid.chord.data.ChordSessionImpl
import space.foid.chord.notify.ChordNotifications
import space.foid.chord.secure.KeystoreCredentialStore
import space.foid.chord.service.ServiceControlImpl
import space.foid.chord.ui.avatar.AvatarRepository
import space.foid.chord.ui.sheets.EmojiCatalog
import java.io.File

/** The application. It holds the one [ChordSession] of the process (manual DI, no Hilt). */
class ChordApp : Application() {
    /** Work that lives as long as the process. */
    val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    /** The signed-in account. Created on first use. */
    val session: ChordSession by lazy {
        ChordSessionImpl(
            accountsDir = File(filesDir, "accounts"),
            credentials = KeystoreCredentialStore(this),
            service = ServiceControlImpl(this),
        )
    }

    /** Avatar bitmaps. It clears itself when the session ends. */
    val avatars: AvatarRepository by lazy { AvatarRepository.forSession(session, appScope) }

    override fun onCreate() {
        super.onCreate()
        // Before any login or upload: the DNS resolver and the certificate verifier need it.
        NativeInit.init(this)
        ChordNotifications.createChannels(this)
        // The reaction picker then opens at once.
        appScope.launch { EmojiCatalog.load(this@ChordApp) }
    }
}
