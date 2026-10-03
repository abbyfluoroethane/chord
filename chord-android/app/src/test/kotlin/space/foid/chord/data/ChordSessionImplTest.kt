package space.foid.chord.data

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
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
private class FakeClient(val log: MutableList<String>, val failLogin: () -> Exception? = { null }) : ChordClient(NoHandle) {
    var listener: ClientEventListener? = null

    override fun subscribeEvents(listener: ClientEventListener): Subscription {
        log += "subscribe"
        this.listener = listener
        return Subscription(NoHandle)
    }

    override suspend fun login(jid: String, password: String, server: String) {
        log += "login $jid $server"
        listener?.onEvent(ClientEvent.ConnectionState(ConnectionState.Connecting))
        failLogin()?.let { throw it }
        listener?.onEvent(ClientEvent.ConnectionState(ConnectionState.Connected("$jid/res", false)))
    }

    override suspend fun logout() {
        log += "logout"
    }

    override fun close() {
        log += "close"
    }
}

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class ChordSessionImplTest {
    @get:Rule val tmp = TemporaryFolder()

    private val log = ArrayList<String>()
    private val store = MemoryCredentialStore()
    private val service = RecordingServiceControl()
    private var paths = ArrayList<String>()
    private var failLogin: Exception? = null
    private var loginFailures: () -> Exception? = { failLogin }

    private fun session(scope: CoroutineScope = CoroutineScope(Dispatchers.Unconfined)) = ChordSessionImpl(
        accountsDir = tmp.root.resolve("accounts"),
        credentials = store,
        service = service,
        io = Dispatchers.Unconfined,
        scope = scope,
        clientFactory = { path, _ ->
            paths += path
            FakeClient(log, loginFailures)
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
    fun restoreOpensTheClientAtOnceAndLogsInInTheBackground() = runTest {
        store.saved = Credentials("bob@x.org", "secret", "starttls://h:5222")
        val s = session(backgroundScope)
        assertTrue(s.restore())
        // Published before any login ran: the cached data shows with no network.
        assertNotNull(s.client.value)
        assertEquals(listOf("subscribe"), log)
        assertEquals(listOf("start"), service.calls)
        runCurrent()
        assertTrue(log.contains("login bob@x.org starttls://h:5222"))
        assertEquals(ConnectionState.Connected("bob@x.org/res", false), s.connection.value)
    }

    @Test
    fun aNetworkFailureKeepsTheUserInAndRetries() = runTest {
        store.saved = Credentials("bob@x.org", "secret", "")
        var tries = 0
        loginFailures = { if (tries++ < 2) ChordException.Unreachable("no network") else null }
        val s = session(backgroundScope)
        assertTrue(s.restore())
        runCurrent()
        assertNotNull(s.client.value)
        assertEquals(1, tries)
        advanceTimeBy(60_000)
        runCurrent()
        assertEquals(3, tries)
        assertNotNull(s.client.value)
        assertNotNull(store.saved)
        assertEquals(ConnectionState.Connected("bob@x.org/res", false), s.connection.value)
        assertEquals(listOf("start"), service.calls)
    }

    @Test
    fun anAuthFailureSendsTheUserToSignIn() = runTest {
        store.saved = Credentials("bob@x.org", "secret", "")
        loginFailures = { ChordException.AuthFailed("bad password") }
        val s = session(backgroundScope)
        assertTrue(s.restore())
        runCurrent()
        assertNull(s.client.value)
        assertNull(store.saved)
        assertEquals(listOf("start", "stop"), service.calls)
        assertEquals("close", log.last())
    }

    @Test
    fun restoreTwiceKeepsTheSameClient() = runTest {
        store.saved = Credentials("bob@x.org", "secret", "")
        val s = session(backgroundScope)
        assertTrue(s.restore())
        val first = s.client.value
        assertTrue(s.restore())
        assertTrue(first === s.client.value)
        assertEquals(listOf("start"), service.calls)
    }

    @Test
    fun signOutStopsTheBackgroundLogin() = runTest {
        store.saved = Credentials("bob@x.org", "secret", "")
        var tries = 0
        loginFailures = { tries++; ChordException.Unreachable("no network") }
        val s = session(backgroundScope)
        s.restore()
        runCurrent()
        s.signOut()
        advanceTimeBy(120_000)
        runCurrent()
        assertEquals(1, tries)
        assertNull(s.client.value)
        assertNull(store.saved)
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
