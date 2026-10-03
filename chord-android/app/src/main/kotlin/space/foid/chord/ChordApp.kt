package space.foid.chord

import android.app.Application
import space.foid.chord.data.ChordSession
import space.foid.chord.data.ChordSessionImpl
import space.foid.chord.data.CredentialStore
import space.foid.chord.data.Credentials
import space.foid.chord.data.ServiceControl
import java.io.File

/**
 * The application. It holds the one [ChordSession] of the process (manual DI, no Hilt).
 *
 * The constructor parameters let a test or the integration replace the credential store
 * and the service control. Android needs a no-argument constructor, and it gets one
 * from the default values.
 */
class ChordApp(
    credentialStore: CredentialStore? = null,
    serviceControl: ServiceControl? = null,
) : Application() {
    // TODO(integration): use secure/KeystoreCredentialStore and the real service control.
    private val credentialStore: CredentialStore = credentialStore ?: InMemoryCredentialStore()
    private val serviceControl: ServiceControl = serviceControl ?: NoopServiceControl

    /** The signed-in account. Created on first use. */
    val session: ChordSession by lazy {
        ChordSessionImpl(
            accountsDir = File(filesDir, "accounts"),
            credentials = this.credentialStore,
            service = this.serviceControl,
        )
    }
}

/** Temporary: keeps the credentials for the life of the process only. */
private class InMemoryCredentialStore : CredentialStore {
    @Volatile private var saved: Credentials? = null
    override fun save(jid: String, password: String, server: String) {
        saved = Credentials(jid, password, server)
    }
    override fun load(): Credentials? = saved
    override fun clear() {
        saved = null
    }
}

/** Temporary: does nothing. */
private object NoopServiceControl : ServiceControl {
    override fun start() = Unit
    override fun stop() = Unit
}
