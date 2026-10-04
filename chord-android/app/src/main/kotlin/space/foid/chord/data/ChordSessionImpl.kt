package space.foid.chord.data

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.chord_ffi.ChordException
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
import kotlinx.coroutines.currentCoroutineContext
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
 * [restore] works offline first: it opens the store of the saved account, publishes the client
 * and starts the service at once, then logs in in the background. A network failure retries
 * with a growing delay. Only a real auth failure ends the session (see [authFailed]).
 *
 * @param accountsDir the directory with one store `<jid>.db` for each account
 * @param clientFactory opens a client for a database path and an account. A test replaces it.
 */
class ChordSessionImpl(
    private val accountsDir: File,
    private val credentials: CredentialStore,
    private val service: ServiceControl,
    private val io: CoroutineDispatcher = Dispatchers.IO,
    private val scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
    private val clientFactory: (dbPath: String, account: String) -> ChordClient =
        { path, account -> ChordClient(path, account) },
) : ChordSession {
    private val mutex = Mutex()
    private var eventSub: Subscription? = null
    private var connectJob: Job? = null

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
        mutex.withLock {
            if (_client.value != null) return true
            val newClient = withContext(io) {
                val db = dbFile(saved.jid)
                db.parentFile?.mkdirs()
                clientFactory(db.absolutePath, saved.jid)
            }
            eventSub = newClient.subscribeEvents(listener)
            _client.value = newClient
            service.start()
            connectJob = scope.launch { connectInBackground(newClient, saved) }
        }
        return true
    }

    /** Log in until it works. A network failure waits and retries. An auth failure ends the session. */
    private suspend fun connectInBackground(client: ChordClient, saved: Credentials) {
        var wait = RETRY_FIRST_MS
        while (currentCoroutineContext().isActive && _client.value === client) {
            try {
                client.login(saved.jid, saved.password, saved.server)
                return
            } catch (e: CancellationException) {
                throw e
            } catch (e: ChordException.AuthFailed) {
                logWarn(TAG, "the saved credentials are not valid", e)
                authFailed(client)
                return
            } catch (e: Exception) {
                logWarn(TAG, "background login failed, retry in ${wait}ms", e)
            }
            delay(wait)
            wait = minOf(wait * 2, RETRY_MAX_MS)
        }
    }

    /** The server rejected the saved credentials: forget them, close the client and stop the service. */
    private suspend fun authFailed(client: ChordClient) {
        withContext(NonCancellable) {
            mutex.withLock {
                if (_client.value !== client) return@withLock
                teardown(logout = false)
                runCatching { withContext(io) { credentials.clear() } }
                    .onFailure { logWarn(TAG, "could not clear the credentials", it) }
                service.stop()
            }
        }
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
        connectJob?.cancel()
        connectJob = null
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
        const val RETRY_FIRST_MS = 2_000L
        const val RETRY_MAX_MS = 20_000L
        val UNSAFE_FILE_CHARS = Regex("[^A-Za-z0-9@._-]")
    }
}
