package space.foid.chord.viewmodel

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.data.ChordSession
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.ConnectFailure
import uniffi.chord_ffi.ConnectionState

private class FakeSession : ChordSession {
    override val client: StateFlow<ChordClient?> = MutableStateFlow(null)
    override val connection: StateFlow<ConnectionState> = MutableStateFlow(ConnectionState.Disconnected)
    override val events: SharedFlow<ClientEvent> = MutableSharedFlow()
    val signIns = ArrayList<Triple<String, String, String>>()
    var gate: CompletableDeferred<Unit>? = null
    var failure: Exception? = null

    override suspend fun signIn(jid: String, password: String, server: String) {
        signIns += Triple(jid, password, server)
        gate?.await()
        failure?.let { throw it }
    }

    override suspend fun restore() = false
    override suspend fun signOut() = Unit
}

@OptIn(ExperimentalCoroutinesApi::class)
class SignInViewModelTest {
    @Test
    fun cannotSubmitAnEmptyForm() = runTest {
        val vm = SignInViewModel(FakeSession(), backgroundScope)
        assertFalse(vm.state.value.canSubmit)
        vm.onJidChange("a@b.c")
        assertFalse(vm.state.value.canSubmit)
        vm.onPasswordChange("pw")
        assertTrue(vm.state.value.canSubmit)
    }

    @Test
    fun anAddressWithoutAtSignGetsAMessage() = runTest {
        val session = FakeSession()
        val vm = SignInViewModel(session, backgroundScope)
        vm.onJidChange("alice")
        vm.onPasswordChange("pw")
        vm.submit()
        runCurrent()
        assertEquals("Enter your address like you@example.com.", vm.state.value.error)
        assertTrue(session.signIns.isEmpty())
    }

    @Test
    fun submitSignsInWithTrimmedValues() = runTest {
        val session = FakeSession()
        val vm = SignInViewModel(session, backgroundScope)
        vm.onJidChange("  alice@x.org ")
        vm.onPasswordChange(" pw ")
        vm.onServerChange(" host ")
        vm.submit()
        runCurrent()
        assertEquals(listOf(Triple("alice@x.org", " pw ", "host")), session.signIns)
        assertTrue(vm.state.value.signedIn)
        assertFalse(vm.state.value.submitting)
        assertEquals("", vm.state.value.password)
    }

    @Test
    fun aSecondSubmitWaitsForTheFirst() = runTest {
        val session = FakeSession()
        session.gate = CompletableDeferred()
        val vm = SignInViewModel(session, backgroundScope)
        vm.onJidChange("a@b.c")
        vm.onPasswordChange("pw")
        vm.submit()
        vm.submit()
        runCurrent()
        assertTrue(vm.state.value.submitting)
        assertEquals(1, session.signIns.size)
        session.gate!!.complete(Unit)
        runCurrent()
        assertFalse(vm.state.value.submitting)
    }

    @Test
    fun aFailureShowsPlainEnglishAndAllowsARetry() = runTest {
        val session = FakeSession()
        session.failure = ChordException.AuthFailed("wrong username or password")
        val vm = SignInViewModel(session, backgroundScope)
        vm.onJidChange("a@b.c")
        vm.onPasswordChange("pw")
        vm.submit()
        runCurrent()
        val s = vm.state.value
        assertFalse(s.submitting)
        assertFalse(s.signedIn)
        assertEquals("Wrong address or password.", s.error)
        vm.onPasswordChange("pw2")
        assertNull(vm.state.value.error)
        session.failure = null
        vm.submit()
        runCurrent()
        assertTrue(vm.state.value.signedIn)
    }

    @Test
    fun signInErrorTextsUseTheDesktopWords() {
        assertEquals("Can't reach x.org. Check the address and your connection.", signInErrorText(ChordException.Unreachable("dns"), "x.org"))
        assertEquals("The certificate of x.org is not valid, so Chord did not connect.", signInErrorText(ChordException.TlsInvalid("c"), "x.org"))
        assertEquals("x.org did not answer in time. Try again.", signInErrorText(ChordException.Timeout(), "x.org"))
        assertEquals("This account is disabled. Ask the people who run your server.", signInErrorText(ChordException.AuthFailed("account disabled"), ""))
        assertEquals("Your password has expired. Set a new one on your server, then sign in again.", signInErrorText(ChordException.AuthFailed("password expired"), ""))
        assertEquals("The server refused the sign-in (temporary auth failure).", signInErrorText(ChordException.AuthFailed("server rejected the login: TemporaryAuthFailure"), ""))
        assertEquals("The server offers no sign-in method that Chord can use.", signInErrorText(ChordException.AuthFailed("no common SASL mechanism"), ""))
        assertEquals("the server did not answer in time. Try again.", signInErrorText(ChordException.Timeout(), ""))
    }

    @Test
    fun theStatusLineNamesTheHost() {
        assertEquals("foid.space", SignInState(jid = "a@foid.space").host)
        assertEquals("chat.example.com", SignInState(jid = "a@foid.space", server = "starttls://chat.example.com:5222").host)
        assertEquals("chat.example.com", SignInState(jid = "a@foid.space", server = "chat.example.com:5222").host)
        assertEquals("", SignInState(jid = "a").host)
    }

    @Test
    fun anEmptyPasswordGetsAMessage() = runTest {
        val vm = SignInViewModel(FakeSession(), backgroundScope)
        vm.onJidChange("a@b.c")
        vm.submit()
        runCurrent()
        // canSubmit is false, but submit() itself still guards.
        assertEquals("Enter your password.", vm.state.value.error)
    }

    @Test
    fun errorTexts() {
        assertTrue(describeError(ChordException.Unreachable("dns")).contains("reach"))
        assertTrue(describeError(ChordException.TlsInvalid("x")).contains("certificate"))
        assertTrue(describeError(ChordException.Timeout()).contains("too long"))
        assertTrue(describeError(ChordException.InvalidServer("x")).contains("server setting"))
        assertTrue(describeError(ChordException.InvalidJid("x")).contains("address"))
        assertEquals("Something went wrong. Try again.", describeError(RuntimeException("x")))
        assertTrue(describeFailure(ConnectFailure.Timeout).contains("too long"))
        // Details of the core stay out of the text.
        assertFalse(describeError(ChordException.Server("secret detail")).contains("secret"))
    }
}
