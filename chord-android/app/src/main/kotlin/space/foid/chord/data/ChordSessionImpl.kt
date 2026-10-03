package space.foid.chord.data

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.ClientEventListener
import uniffi.chord_ffi.ConnectionState
import uniffi.chord_ffi.Subscription
import java.io.File

/**
 * The [ChordSession] on the real core. It holds no protocol logic: it opens the store, logs
 * in, and publishes the client, the connection state and the events.
 *
 * Sign-in order: open the client, subscribe to the events, then log in. So no connection
 * state or event of the login is missed. The client is published only after the login
 * succeeded. A failed login closes the client again and rethrows the error.
 *
 * @param accountsDir the directory with one store `<jid>.db` for each account
 * @param clientFactory opens a client for a database path and an account. A test replaces it.
 */
class ChordSessionImpl(
    private val accountsDir: File,
    private val credentials: CredentialStore,
    private val service: ServiceControl,
    private val io: CoroutineDispatcher = Dispatchers.IO,
    private val clientFactory: (dbPath: String, account: String) -> ChordClient =
        { path, account -> ChordClient(path, account) },
) : ChordSession {
    private val mutex = Mutex()
    private var eventSub: Subscription? = null

    private val _client = MutableStateFlow<ChordClient?>(null)
    override val client: StateFlow<ChordClient?> = _client.asStateFlow()

    private val _connection = MutableStateFlow<ConnectionState>(ConnectionState.Disconnected)
    override val connection: StateFlow<ConnectionState> = _connection.asStateFlow()

    // The core thread must never block: the buffer drops the oldest event when it is full.
    private val _events = MutableSharedFlow<ClientEvent>(
        extraBufferCapacity = EVENT_BUFFER,
        onBufferOverflow = kotlinx.coroutines.channels.BufferOverflow.DROP_OLDEST,
    )
    override val events: SharedFlow<ClientEvent> = _events.asSharedFlow()

    private val listener = object : ClientEventListener {
        override fun onEvent(event: ClientEvent) {
            if (event is ClientEvent.ConnectionState) _connection.value = event.state
            _events.tryEmit(event)
        }
    }

    override suspend fun signIn(jid: String, password: String, server: String) {
        val account = jid.trim()
        require(account.isNotEmpty()) { "empty JID" }
        mutex.withLock {
            teardown(logout = false)
            val db = dbFile(account)
            val newClient = withContext(io) {
                db.parentFile?.mkdirs()
                clientFactory(db.absolutePath, account)
            }
            var sub: Subscription? = null
            try {
                sub = newClient.subscribeEvents(listener)
                newClient.login(account, password, server)
                withContext(io) { credentials.save(account, password, server) }
            } catch (e: Throwable) {
                withContext(NonCancellable) {
                    runCatching { sub?.cancel(); sub?.close() }
                    runCatching { newClient.close() }
                    _connection.value = ConnectionState.Disconnected
                }
                throw e
            }
            eventSub = sub
            _client.value = newClient
            service.start()
        }
    }

    override suspend fun restore(): Boolean {
        val saved = withContext(io) { credentials.load() } ?: return false
        signIn(saved.jid, saved.password, saved.server)
        return true
    }

    override suspend fun signOut() {
        mutex.withLock {
            withContext(NonCancellable) {
                teardown(logout = true)
                runCatching { withContext(io) { credentials.clear() } }
                    .onFailure { logWarn(TAG, "could not clear the credentials", it) }
                service.stop()
            }
        }
    }

    /** Log out (if asked), cancel the subscription and close the client. Call it with the mutex held. */
    private suspend fun teardown(logout: Boolean) {
        val old = _client.value
        val sub = eventSub
        eventSub = null
        _client.value = null
        if (old != null && logout) {
            runCatching { old.logout() }.onFailure { logWarn(TAG, "logout failed", it) }
        }
        runCatching { sub?.cancel(); sub?.close() }
        if (old != null) runCatching { old.close() }
        _connection.value = ConnectionState.Disconnected
    }

    private fun dbFile(account: String): File =
        File(accountsDir, account.replace(UNSAFE_FILE_CHARS, "_") + ".db")

    private companion object {
        const val TAG = "ChordSession"
        const val EVENT_BUFFER = 256
        val UNSAFE_FILE_CHARS = Regex("[^A-Za-z0-9@._-]")
    }
}
