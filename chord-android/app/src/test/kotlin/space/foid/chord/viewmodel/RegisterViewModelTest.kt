package space.foid.chord.viewmodel

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.data.ChordSession
import space.foid.chord.ui.forms.registrationFormWithCaptcha
import space.foid.chord.ui.forms.withValues
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.ConnectionState
import uniffi.chord_ffi.DataFormKind
import uniffi.chord_ffi.RegistrationAnswer
import uniffi.chord_ffi.RegistrationInfo

private class FakeRegistration : RegistrationApi {
    var info = RegistrationInfo("Pick a name", null, listOf("username", "password"), null, false)
    var formFailure: Exception? = null
    var registerFailure: Exception? = null
    val forms = ArrayList<Pair<String, String>>()
    val registered = ArrayList<Triple<String, String, RegistrationAnswer>>()

    override suspend fun form(domain: String, server: String): RegistrationInfo {
        forms += domain to server
        formFailure?.let { throw it }
        return info
    }

    override suspend fun register(domain: String, server: String, answer: RegistrationAnswer) {
        registered += Triple(domain, server, answer)
        registerFailure?.let { throw it }
    }
}

private class RecordingSession : ChordSession {
    override val client: StateFlow<ChordClient?> = MutableStateFlow(null)
    override val connection: StateFlow<ConnectionState> = MutableStateFlow(ConnectionState.Disconnected)
    override val events: SharedFlow<ClientEvent> = MutableSharedFlow()
    val signIns = ArrayList<Triple<String, String, String>>()
    var failure: Exception? = null

    override suspend fun signIn(jid: String, password: String, server: String) {
        signIns += Triple(jid, password, server)
        failure?.let { throw it }
    }

    override suspend fun restore() = false
    override suspend fun signOut() = Unit
}

@OptIn(ExperimentalCoroutinesApi::class)
class RegisterViewModelTest {
    @Test
    fun theServerStepStartsWithTheDomainOfTheAddress() = runTest {
        val vm = RegisterViewModel(FakeRegistration(), RecordingSession(), "Rin@Foid.Space", "", backgroundScope)
        assertEquals("foid.space", vm.state.value.domain)
        assertTrue(vm.state.value.canContinue)
        vm.onDomainChange("  ")
        assertFalse(vm.state.value.canContinue)
    }

    @Test
    fun continueAsksTheServerAndShowsTheForm() = runTest {
        val api = FakeRegistration()
        val vm = RegisterViewModel(api, RecordingSession(), "", "starttls://h:5222", backgroundScope)
        vm.onDomainChange(" Example.COM ")
        vm.fetchForm()
        runCurrent()
        assertEquals(listOf("example.com" to "starttls://h:5222"), api.forms)
        val s = vm.state.value
        assertEquals(RegisterStep.FORM, s.step)
        assertEquals("example.com", s.domain)
        assertFalse(s.busy)
        assertFalse(s.linkOnly)
    }

    @Test
    fun aFailedFormShowsTheErrorAndStaysOnTheServerStep() = runTest {
        val api = FakeRegistration().apply { formFailure = ChordException.Unreachable("dns") }
        val vm = RegisterViewModel(api, RecordingSession(), "a@b.c", "", backgroundScope)
        vm.fetchForm()
        runCurrent()
        assertEquals(RegisterStep.SERVER, vm.state.value.step)
        assertEquals("Can't reach the server. Check the address and your connection.", vm.state.value.error)
        vm.onDomainChange("b.d")
        assertNull(vm.state.value.error)
    }

    @Test
    fun emptyLegacyFieldsGetTheDesktopMessage() = runTest {
        val api = FakeRegistration()
        val vm = RegisterViewModel(api, RecordingSession(), "a@b.c", "", backgroundScope)
        vm.fetchForm()
        runCurrent()
        vm.submit()
        runCurrent()
        assertEquals("Fill in: Username, Password.", vm.state.value.error)
        assertTrue(api.registered.isEmpty())
    }

    @Test
    fun legacyRegistrationThenSignIn() = runTest {
        val api = FakeRegistration()
        val session = RecordingSession()
        val vm = RegisterViewModel(api, session, "a@b.c", "srv", backgroundScope)
        vm.fetchForm()
        runCurrent()
        vm.onLegacyChange("username", " rin ")
        vm.onLegacyChange("password", "pw")
        vm.submit()
        runCurrent()
        val (domain, server, answer) = api.registered.single()
        assertEquals("b.c", domain)
        assertEquals("srv", server)
        assertNull(answer.form)
        assertEquals(listOf("username" to "rin", "password" to "pw"), answer.fields.map { it.name to it.value })
        assertEquals(listOf(Triple("rin@b.c", "pw", "srv")), session.signIns)
        assertTrue(vm.state.value.signedIn)
    }

    @Test
    fun aDataFormShowsProblemsThenSendsASubmission() = runTest {
        val api = FakeRegistration().apply { info = RegistrationInfo(null, registrationFormWithCaptcha(), emptyList(), null, false) }
        val session = RecordingSession()
        val vm = RegisterViewModel(api, session, "a@foid.space", "", backgroundScope)
        vm.fetchForm()
        runCurrent()
        vm.submit()
        runCurrent()
        assertEquals("User is required", vm.state.value.error)
        assertTrue(vm.state.value.showProblems)
        assertTrue(api.registered.isEmpty())

        var f = vm.state.value.form!!
        f = f.withValues(1, listOf("rin")).withValues(2, listOf("pw")).withValues(3, listOf("xk7"))
        vm.onFormChange(f)
        vm.submit()
        runCurrent()
        val sent = api.registered.single().third.form
        assertNotNull(sent)
        assertEquals(DataFormKind.SUBMIT, sent!!.kind)
        assertEquals(listOf("xk7"), sent.fields[3].values)
        assertEquals(listOf(Triple("rin@foid.space", "pw", "")), session.signIns)
        assertTrue(vm.state.value.signedIn)
    }

    @Test
    fun aClosedServerShowsHowToGoOn() = runTest {
        val api = FakeRegistration().apply { registerFailure = ChordException.Server("Forbidden") }
        val session = RecordingSession()
        val vm = RegisterViewModel(api, session, "a@b.c", "", backgroundScope)
        vm.fetchForm()
        runCurrent()
        vm.onLegacyChange("username", "rin")
        vm.onLegacyChange("password", "pw")
        vm.submit()
        runCurrent()
        assertTrue(vm.state.value.error!!.contains("invite link"))
        assertFalse(vm.state.value.busy)
        assertTrue(session.signIns.isEmpty())
    }

    @Test
    fun aFailedSignInAfterTheRegistrationSaysSo() = runTest {
        val api = FakeRegistration()
        val session = RecordingSession().apply { failure = ChordException.Timeout() }
        val vm = RegisterViewModel(api, session, "a@b.c", "", backgroundScope)
        vm.fetchForm()
        runCurrent()
        vm.onLegacyChange("username", "rin")
        vm.onLegacyChange("password", "pw")
        vm.submit()
        runCurrent()
        assertTrue(vm.state.value.error!!.startsWith("Your account was created"))
        assertFalse(vm.state.value.signedIn)
        assertFalse(vm.state.value.signingIn)
    }

    @Test
    fun aLinkOnlyServerHasNoFields() = runTest {
        val api = FakeRegistration().apply {
            info = RegistrationInfo(null, null, emptyList(), uniffi.chord_ffi.OobLink("https://x.example/join", null), false)
        }
        val vm = RegisterViewModel(api, RecordingSession(), "a@b.c", "", backgroundScope)
        vm.fetchForm()
        runCurrent()
        assertTrue(vm.state.value.linkOnly)
    }

    @Test
    fun backGoesFromTheFormToTheServerStepThenLeaves() = runTest {
        val vm = RegisterViewModel(FakeRegistration(), RecordingSession(), "a@b.c", "", backgroundScope)
        assertFalse(vm.back())
        vm.fetchForm()
        runCurrent()
        assertTrue(vm.back())
        assertEquals(RegisterStep.SERVER, vm.state.value.step)
        assertNull(vm.state.value.info)
        assertFalse(vm.back())
    }
}
