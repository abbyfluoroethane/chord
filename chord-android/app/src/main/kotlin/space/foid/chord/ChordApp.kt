package space.foid.chord

import android.app.Application
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import space.foid.chord.data.logWarn
import kotlinx.coroutines.launch
import space.foid.chord.data.ChordSession
import space.foid.chord.data.ChordSessionImpl
import space.foid.chord.notify.ChordNotifications
import space.foid.chord.secure.KeystoreCredentialStore
import space.foid.chord.ui.inbox.InboxEvents
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

    /**
     * The offline-first start, running from [onCreate]: load the native library, then open the
     * saved account with no network (see [ChordSession.restore]). It completes with true when a
     * client is open, and with false when there are no saved credentials or the store cannot
     * open. The activity and the service wait for it. It never waits for the network.
     */
    lateinit var startup: Deferred<Boolean>
        private set

    override fun onCreate() {
        super.onCreate()
        Startup.mark("app.onCreate")
        // Off the main thread. Before any login or upload: the DNS resolver and the certificate
        // verifier need it. The restore runs after it in the same job, so a login never comes first.
        startup = appScope.async {
            NativeInit.init(this@ChordApp)
            Startup.mark("native.init")
            // Before the login: the core sends pending requests and invites right after it.
            InboxEvents.attach(session.events, session.client)
            val opened = try {
                session.restore()
            } catch (e: Exception) {
                logWarn("ChordApp", "the saved account did not open", e)
                false
            }
            Startup.mark("session.restored=$opened")
            opened
        }
        ChordNotifications.createChannels(this)
        appScope.launch {
            // The reaction picker then opens at once. After the first frame, to leave the CPU to start-up.
            delay(EMOJI_DELAY_MS)
            EmojiCatalog.load(this@ChordApp)
        }
    }

    private companion object {
        const val EMOJI_DELAY_MS = 1500L
    }
}
