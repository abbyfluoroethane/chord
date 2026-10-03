package space.foid.chord.viewmodel

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.ui.settings.SettingsStore
import space.foid.chord.ui.settings.ThemeMode
import uniffi.chord_ffi.Availability
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.InvisibleMethod
import uniffi.chord_ffi.OwnPresence

internal class FakeSettingsApi : SettingsApi {
    val calls = ArrayList<String>()
    var nickname: String? = "Ally"
    var avatar: ByteArray? = null
    var presence = OwnPresence(Availability.AWAY, "lunch")
    var hide: InvisibleMethod? = InvisibleMethod.COMMAND
    var blocked = mutableListOf("spam@example.org", "bot@example.org")
    var failWith: Exception? = null
    var failReads = false

    private fun write(call: String) {
        failWith?.let { throw it }
        calls += call
    }

    private fun <T> read(value: T): T {
        if (failReads) throw ChordException.NotConnected()
        return value
    }

    override fun account() = "me@example.org"
    override suspend fun nickname() = read(nickname)
    override suspend fun setNickname(nickname: String?) = write("setNickname $nickname")
    override suspend fun avatar() = read(avatar)
    override suspend fun setAvatar(mime: String, data: ByteArray, width: Int, height: Int) =
        write("setAvatar $mime ${data.size} ${width}x$height")
    override suspend fun removeAvatar() = write("removeAvatar")
    override suspend fun ownPresence() = read(presence)
    override suspend fun invisibleMethod() = read(hide)
    override suspend fun setPresence(availability: Availability, status: String?) =
        write("setPresence $availability $status")
    override suspend fun blockedContacts() = read(blocked.toList())
    override suspend fun unblockContact(jid: String) = write("unblock $jid")
    override suspend fun setShareInfo(share: Boolean) = write("shareInfo $share")
}

internal class FakeSettingsStore : SettingsStore {
    override val themeMode = MutableStateFlow(ThemeMode.System)
    override fun setThemeMode(mode: ThemeMode) { themeMode.value = mode }
    override val shareInfo = MutableStateFlow(true)
    override fun setShareInfo(share: Boolean) { shareInfo.value = share }
}

@OptIn(ExperimentalCoroutinesApi::class)
class SettingsViewModelTest {
    private fun TestScope.vm(
        api: FakeSettingsApi = FakeSettingsApi(),
        store: FakeSettingsStore = FakeSettingsStore(),
        signOut: suspend () -> Unit = {},
    ): SettingsViewModel {
        val v = SettingsViewModel(api, store, signOut, backgroundScope)
        runCurrent()
        return v
    }

    @Test
    fun loadsTheAccountData() = runTest {
        val v = vm()
        val s = v.state.value
        assertTrue(s.loaded)
        assertEquals("me@example.org", s.jid)
        assertEquals("Ally", s.nickname)
        assertEquals("Ally", s.nicknameDraft)
        assertEquals(Availability.AWAY, s.availability)
        assertEquals("lunch", s.status)
        assertTrue(s.canHide)
        assertEquals(listOf("spam@example.org", "bot@example.org"), s.blocked)
    }

    @Test
    fun aFailedReadStillLoadsTheRest() = runTest {
        val api = FakeSettingsApi().apply { failReads = true }
        val s = vm(api).state.value
        assertTrue(s.loaded)
        assertEquals("", s.nickname)
        assertFalse(s.canHide)
        assertNull(s.error)
    }

    @Test
    fun savingTheNicknamePublishesTheTrimmedText() = runTest {
        val api = FakeSettingsApi()
        val v = vm(api)
        v.onNicknameChange("  Bee ")
        assertTrue(v.state.value.nicknameChanged)
        v.saveNickname()
        runCurrent()
        assertEquals(listOf("setNickname Bee"), api.calls)
        assertEquals("Bee", v.state.value.nickname)
        assertFalse(v.state.value.nicknameChanged)
    }

    @Test
    fun anEmptyNicknameRemovesIt() = runTest {
        val api = FakeSettingsApi()
        val v = vm(api)
        v.onNicknameChange("")
        v.saveNickname()
        runCurrent()
        assertEquals(listOf("setNickname null"), api.calls)
    }

    @Test
    fun anUnchangedNicknameIsNotSent() = runTest {
        val api = FakeSettingsApi()
        val v = vm(api)
        v.saveNickname()
        runCurrent()
        assertTrue(api.calls.isEmpty())
    }

    @Test
    fun aFailedSaveShowsAMessageAndKeepsTheDraft() = runTest {
        val api = FakeSettingsApi()
        val v = vm(api)
        api.failWith = ChordException.NotConnected()
        v.onNicknameChange("Bee")
        v.saveNickname()
        runCurrent()
        assertEquals("You are not connected to the server.", v.state.value.error)
        assertEquals("Ally", v.state.value.nickname)
        assertEquals("Bee", v.state.value.nicknameDraft)
        v.dismissError()
        assertNull(v.state.value.error)
    }

    @Test
    fun changingTheAvailabilityKeepsTheStatusText() = runTest {
        val api = FakeSettingsApi()
        val v = vm(api)
        v.setAvailability(Availability.DND)
        runCurrent()
        assertEquals(listOf("setPresence DND lunch"), api.calls)
        assertEquals(Availability.DND, v.state.value.availability)
    }

    @Test
    fun savingTheStatusKeepsTheAvailability() = runTest {
        val api = FakeSettingsApi()
        val v = vm(api)
        v.onStatusChange("back at 3 ")
        v.saveStatus()
        runCurrent()
        assertEquals(listOf("setPresence AWAY back at 3"), api.calls)
        assertEquals("back at 3", v.state.value.status)
    }

    @Test
    fun invisibleIsOfferedOnlyWhenTheServerCanHideUs() = runTest {
        val can = vm().state.value
        assertTrue(Availability.INVISIBLE in can.availabilities)
        val api = FakeSettingsApi().apply { hide = null }
        val cannot = vm(api).state.value
        assertFalse(Availability.INVISIBLE in cannot.availabilities)
        assertEquals(listOf(Availability.AVAILABLE, Availability.AWAY, Availability.DND), cannot.availabilities)
    }

    @Test
    fun unblockRemovesTheContactFromTheList() = runTest {
        val api = FakeSettingsApi()
        val v = vm(api)
        v.unblock("spam@example.org")
        runCurrent()
        assertEquals(listOf("unblock spam@example.org"), api.calls)
        assertEquals(listOf("bot@example.org"), v.state.value.blocked)
    }

    @Test
    fun theAvatarIsSetAndRemoved() = runTest {
        val api = FakeSettingsApi()
        val v = vm(api)
        v.setAvatar("image/png", ByteArray(3), 256, 256)
        runCurrent()
        assertEquals("setAvatar image/png 3 256x256", api.calls.single())
        assertEquals(3, v.state.value.avatar?.size)
        v.removeAvatar()
        runCurrent()
        assertNull(v.state.value.avatar)
    }

    @Test
    fun theThemeGoesToTheStoreAndBack() = runTest {
        val store = FakeSettingsStore()
        val v = vm(store = store)
        v.setTheme(ThemeMode.Dark)
        runCurrent()
        assertEquals(ThemeMode.Dark, store.themeMode.value)
        assertEquals(ThemeMode.Dark, v.state.value.theme)
    }

    @Test
    fun shareInfoGoesToTheStoreAndTheCore() = runTest {
        val api = FakeSettingsApi()
        val store = FakeSettingsStore()
        val v = vm(api, store)
        v.setShareInfo(false)
        runCurrent()
        assertEquals(listOf("shareInfo false"), api.calls)
        assertFalse(store.shareInfo.value)
        assertFalse(v.state.value.shareInfo)
    }

    @Test
    fun aFailedShareInfoCallRestoresTheSwitch() = runTest {
        val api = FakeSettingsApi()
        val store = FakeSettingsStore()
        val v = vm(api, store)
        api.failWith = ChordException.NotConnected()
        v.setShareInfo(false)
        runCurrent()
        assertTrue(store.shareInfo.value)
        assertTrue(v.state.value.error != null)
    }

    @Test
    fun signOutRunsTheActionAndFlagsIt() = runTest {
        var ran = 0
        val v = vm(signOut = { ran++ })
        assertFalse(v.signedOut.value)
        v.signOut()
        runCurrent()
        assertEquals(1, ran)
        assertTrue(v.signedOut.value)
    }

    @Test
    fun aFailedSignOutStaysOnTheScreen() = runTest {
        val v = vm(signOut = { throw ChordException.NotConnected() })
        v.signOut()
        runCurrent()
        assertFalse(v.signedOut.value)
        assertTrue(v.state.value.error != null)
    }
}
