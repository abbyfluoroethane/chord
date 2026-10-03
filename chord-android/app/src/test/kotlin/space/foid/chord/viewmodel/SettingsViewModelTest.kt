package space.foid.chord.viewmodel

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.ui.settings.AppPrefs
import space.foid.chord.ui.settings.MemorySettingsStore
import space.foid.chord.ui.settings.SignInShow
import space.foid.chord.ui.settings.ThemeMode
import uniffi.chord_ffi.Availability
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.OwnPresence

internal class FakeSettingsApi : SettingsApi {
    val calls = ArrayList<String>()
    var nickname: String? = "Ally"
    var avatar: ByteArray? = null
    var presence = OwnPresence(Availability.AWAY, "lunch")
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
    override suspend fun blockedContacts() = read(blocked.toList())
    override suspend fun unblockContact(jid: String) = write("unblock $jid")
    override suspend fun setShareInfo(share: Boolean) = write("shareInfo $share")
    override suspend fun unblockAll() = write("unblockAll")
    override suspend fun changePassword(password: String) = write("changePassword $password")
}

internal class FakeSettingsStore : MemorySettingsStore() {
    override fun persist(prefs: AppPrefs) {}
}

@OptIn(ExperimentalCoroutinesApi::class)
class SettingsViewModelTest {
    private fun TestScope.vm(
        api: FakeSettingsApi = FakeSettingsApi(),
        store: FakeSettingsStore = FakeSettingsStore(),
        signOut: suspend () -> Unit = {},
        savePassword: suspend (String) -> Unit = {},
    ): SettingsViewModel {
        val v = SettingsViewModel(api, store, signOut, savePassword, backgroundScope)
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
        assertEquals(listOf("spam@example.org", "bot@example.org"), s.blocked)
    }

    @Test
    fun aFailedReadStillLoadsTheRest() = runTest {
        val api = FakeSettingsApi().apply { failReads = true }
        val s = vm(api).state.value
        assertTrue(s.loaded)
        assertEquals("", s.nickname)
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
        assertEquals(ThemeMode.Dark, v.state.value.prefs.theme)
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
        assertFalse(v.state.value.prefs.shareInfo)
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

    @Test
    fun theSignInStatusIsAPhoneSettingAndNeverCallsTheCore() = runTest {
        val api = FakeSettingsApi()
        val store = FakeSettingsStore()
        val v = vm(api, store)
        v.setSignInShow(SignInShow.Dnd)
        v.onSignInStatusChange("  deep work ")
        assertTrue(v.state.value.signInStatusChanged)
        v.saveSignInStatus()
        runCurrent()
        assertEquals(SignInShow.Dnd, store.prefs.value.signInShow)
        assertEquals("deep work", store.prefs.value.signInStatus)
        assertFalse(v.state.value.signInStatusChanged)
        assertTrue(api.calls.isEmpty())
    }

    @Test
    fun theSignInStatusIsOneLineAndShort() = runTest {
        val v = vm()
        v.onSignInStatusChange("a\nb" + "x".repeat(200))
        val t = v.state.value.signInStatusDraft
        assertEquals(128, t.length)
        assertFalse('\n' in t)
    }

    @Test
    fun resetNicknameRestoresTheSavedName() = runTest {
        val v = vm()
        v.onNicknameChange("Other")
        v.resetNickname()
        assertEquals("Ally", v.state.value.nicknameDraft)
        assertFalse(v.state.value.nicknameChanged)
    }

    @Test
    fun unblockAllEmptiesTheList() = runTest {
        val api = FakeSettingsApi()
        val v = vm(api)
        v.unblockAll()
        runCurrent()
        assertEquals(listOf("unblockAll"), api.calls)
        assertTrue(v.state.value.blocked.isEmpty())
    }

    @Test
    fun prefsGoToTheStoreAndBackIntoTheState() = runTest {
        val store = FakeSettingsStore()
        val v = vm(store = store)
        v.setShowPresence(false)
        v.setShareIdle(false)
        v.setIdleMinutes(30)
        v.setNoticePreview(false)
        v.setQuietHours(true)
        v.setQuietFrom(23 * 60)
        v.setQuietTo(7 * 60 + 30)
        runCurrent()
        val p = v.state.value.prefs
        assertFalse(p.showPresence)
        assertFalse(store.showPresence.value)
        assertFalse(p.shareIdle)
        assertEquals(30, p.idleMinutes)
        assertFalse(p.noticePreview)
        assertTrue(p.quietHours)
        assertEquals(23 * 60, p.quietFrom)
        assertEquals(450, p.quietTo)
    }

    @Test
    fun anIdleWaitOutsideTheChoicesFallsBack() = runTest {
        val store = FakeSettingsStore()
        val v = vm(store = store)
        v.setIdleMinutes(7)
        assertEquals(5, store.prefs.value.idleMinutes)
    }

    @Test
    fun resetPutsEverythingBackAndResetsTheShareInfoAnswer() = runTest {
        val api = FakeSettingsApi()
        val store = FakeSettingsStore()
        val v = vm(api, store)
        v.setTheme(ThemeMode.Dark)
        v.setSignInShow(SignInShow.Away)
        v.setShareInfo(false)
        runCurrent()
        api.calls.clear()
        v.resetSettings()
        runCurrent()
        assertEquals(AppPrefs(), store.prefs.value)
        assertEquals(listOf("shareInfo true"), api.calls)
        assertEquals("", v.state.value.signInStatusDraft)
    }

    @Test
    fun changingThePasswordChecksTheFieldsFirst() = runTest {
        val api = FakeSettingsApi()
        val v = vm(api)
        v.changePassword("", "")
        assertEquals(space.foid.chord.viewmodel.PasswordError.Empty, v.state.value.password.error)
        v.changePassword("one", "two")
        assertEquals(space.foid.chord.viewmodel.PasswordError.Mismatch, v.state.value.password.error)
        runCurrent()
        assertTrue(api.calls.isEmpty())
    }

    @Test
    fun aGoodPasswordGoesToTheServerThenToTheCredentialStore() = runTest {
        val api = FakeSettingsApi()
        val saved = ArrayList<String>()
        val v = vm(api, savePassword = { saved += it })
        v.changePassword("s3cret", "s3cret")
        runCurrent()
        assertEquals(listOf("changePassword s3cret"), api.calls)
        assertEquals(listOf("s3cret"), saved)
        assertTrue(v.state.value.password.done)
        v.clearPassword()
        assertFalse(v.state.value.password.done)
    }

    @Test
    fun aRefusedPasswordIsNotSavedOnThePhone() = runTest {
        val api = FakeSettingsApi()
        val saved = ArrayList<String>()
        val v = vm(api, savePassword = { saved += it })
        api.failWith = ChordException.Server("no")
        v.changePassword("s3cret", "s3cret")
        runCurrent()
        assertTrue(saved.isEmpty())
        val p = v.state.value.password
        assertEquals(space.foid.chord.viewmodel.PasswordError.Server, p.error)
        assertEquals("The server refused the request.", p.message)
        assertFalse(p.busy)
    }

    @Test
    fun aPasswordThatCannotBeSavedIsReported() = runTest {
        val v = vm(savePassword = { throw IllegalStateException("no keystore") })
        v.changePassword("s3cret", "s3cret")
        runCurrent()
        assertEquals(space.foid.chord.viewmodel.PasswordError.SaveFailed, v.state.value.password.error)
        assertFalse(v.state.value.password.done)
    }
}
