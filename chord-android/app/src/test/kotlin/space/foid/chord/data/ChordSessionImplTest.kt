package space.foid.chord.data

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.ClientEventListener
import uniffi.chord_ffi.ConnectionState
import uniffi.chord_ffi.NoHandle
import uniffi.chord_ffi.Subscription

class MemoryCredentialStore : CredentialStore {
    var saved: Credentials? = null
    override fun save(jid: String, password: String, server: String) {
        saved = Credentials(jid, password, server)
    }
    override fun load() = saved
    override fun clear() {
        saved = null
    }
}

class RecordingServiceControl : ServiceControl {
    val calls = ArrayList<String>()
    override fun start() {
        calls += "start"
    }
    override fun stop() {
        calls += "stop"
    }
}

/** A client without the native library: no handle, and every call that the session makes is overridden. */
private class FakeClient(val log: MutableList<String>, val failLogin: Exception? = null) : ChordClient(NoHandle) {
    var listener: ClientEventListener? = null

    override fun subscribeEvents(listener: ClientEventListener): Subscription {
        log += "subscribe"
        this.listener = listener
        return Subscription(NoHandle)
    }

    override suspend fun login(jid: String, password: String, server: String) {
        log += "login $jid $server"
        listener?.onEvent(ClientEvent.ConnectionState(ConnectionState.Connecting))
        failLogin?.let { throw it }
        listener?.onEvent(ClientEvent.ConnectionState(ConnectionState.Connected("$jid/res", false)))
    }

    override suspend fun logout() {
        log += "logout"
    }

    override fun close() {
        log += "close"
    }
}

class ChordSessionImplTest {
    @get:Rule val tmp = TemporaryFolder()

    private val log = ArrayList<String>()
    private val store = MemoryCredentialStore()
    private val service = RecordingServiceControl()
    private var paths = ArrayList<String>()
    private var failLogin: Exception? = null

    private fun session() = ChordSessionImpl(
        accountsDir = tmp.root.resolve("accounts"),
        credentials = store,
        service = service,
        io = Dispatchers.Unconfined,
        clientFactory = { path, _ ->
            paths += path
            FakeClient(log, failLogin)
        },
    )

    @Test
    fun signInSubscribesBeforeLoginAndPublishesTheClient() = runTest {
        val s = session()
        s.signIn("alice@x.org", "pw", "")
        assertEquals(listOf("subscribe", "login alice@x.org "), log)
        assertNotNull(s.client.value)
        assertEquals(ConnectionState.Connected("alice@x.org/res", false), s.connection.value)
        assertEquals(Credentials("alice@x.org", "pw", ""), store.saved)
        assertEquals(listOf("start"), service.calls)
        assertEquals(tmp.root.resolve("accounts/alice@x.org.db").path, paths.single())
        assertTrue(tmp.root.resolve("accounts").isDirectory)
    }

    @Test
    fun aFailedLoginCleansUpAndRethrows() = runTest {
        failLogin = ChordException.AuthFailed("bad")
        val s = session()
        try {
            s.signIn("alice@x.org", "pw", "")
            throw AssertionError("expected an error")
        } catch (e: ChordException.AuthFailed) {
            // expected
        }
        assertNull(s.client.value)
        assertNull(store.saved)
        assertTrue(service.calls.isEmpty())
        assertEquals(ConnectionState.Disconnected, s.connection.value)
        assertEquals("close", log.last())
    }

    @Test
    fun signOutLogsOutClosesClearsAndStops() = runTest {
        val s = session()
        s.signIn("alice@x.org", "pw", "")
        log.clear()
        s.signOut()
        assertEquals(listOf("logout", "close"), log)
        assertNull(s.client.value)
        assertNull(store.saved)
        assertEquals(listOf("start", "stop"), service.calls)
        assertEquals(ConnectionState.Disconnected, s.connection.value)
    }

    @Test
    fun signOutWithNobodySignedInIsHarmless() = runTest {
        val s = session()
        s.signOut()
        assertEquals(listOf("stop"), service.calls)
    }

    @Test
    fun restoreWithoutCredentialsReturnsFalse() = runTest {
        val s = session()
        assertFalse(s.restore())
        assertNull(s.client.value)
    }

    @Test
    fun restoreSignsInWithTheSavedCredentials() = runTest {
        store.saved = Credentials("bob@x.org", "secret", "starttls://h:5222")
        val s = session()
        assertTrue(s.restore())
        assertTrue(log.contains("login bob@x.org starttls://h:5222"))
        assertNotNull(s.client.value)
    }

    @Test
    fun aSecondSignInClosesTheFirstClient() = runTest {
        val s = session()
        s.signIn("alice@x.org", "pw", "")
        log.clear()
        s.signIn("bob@x.org", "pw", "")
        assertEquals(listOf("close", "subscribe", "login bob@x.org "), log)
    }

    @Test
    fun eventsAreForwarded() = runTest {
        val s = session()
        s.signIn("alice@x.org", "pw", "")
        val client = s.client.value as FakeClient
        val seen = ArrayList<ClientEvent>()
        val job = CoroutineScope(Dispatchers.Unconfined).launch { s.events.collect { seen += it } }
        client.listener!!.onEvent(ClientEvent.BlockListChanged)
        assertEquals(listOf<ClientEvent>(ClientEvent.BlockListChanged), seen)
        job.cancel()
    }

    @Test
    fun unsafeCharactersStayOutOfTheFileName() = runTest {
        val s = session()
        s.signIn("../evil/x@y.org", "pw", "")
        val file = java.io.File(paths.single())
        assertEquals(tmp.root.resolve("accounts"), file.parentFile)
    }
}
