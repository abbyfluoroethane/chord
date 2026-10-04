package space.foid.chord.ui.settings

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chord_ffi.Availability
import uniffi.chord_ffi.OwnPresence

private class FakePresence(var stored: OwnPresence) : PresenceApi {
    val calls = ArrayList<String>()
    override suspend fun ownPresence() = stored
    override suspend fun setPresence(availability: Availability, status: String?) {
        calls += "setPresence $availability $status"
    }
    override suspend fun setIdle(since: Long?) {
        calls += "setIdle $since"
    }
}

class SettingsPolicyTest {
    private val stored = OwnPresence(Availability.AWAY, "lunch")

    @Test
    fun lastKeepsTheStoredPresence() {
        assertEquals(stored, signInPresence(stored, AppPrefs()))
    }

    @Test
    fun theChosenAvailabilityGoesOverTheStoredOne() {
        val p = AppPrefs(signInShow = SignInShow.Dnd)
        assertEquals(OwnPresence(Availability.DND, "lunch"), signInPresence(stored, p))
    }

    @Test
    fun aStatusTextGoesOverTheStoredOneAndIsTrimmedAndCut() {
        val p = AppPrefs(signInStatus = "  " + "x".repeat(200))
        val out = signInPresence(stored, p)
        assertEquals(Availability.AWAY, out.availability)
        assertEquals(128, out.status?.length)
    }

    @Test
    fun nothingIsSetWhenThePrefsChangeNothing() = runTest {
        val api = FakePresence(stored)
        assertFalse(applySignInPresence(api, AppPrefs()))
        assertFalse(applySignInPresence(api, AppPrefs(signInShow = SignInShow.Away, signInStatus = "lunch")))
        assertTrue(api.calls.isEmpty())
    }

    @Test
    fun aDifferentPresenceIsSetOnce() = runTest {
        val api = FakePresence(stored)
        assertTrue(applySignInPresence(api, AppPrefs(signInShow = SignInShow.Chat, signInStatus = "hi")))
        assertEquals(listOf("setPresence AVAILABLE hi"), api.calls)
    }

    @Test
    fun quietHoursInOneDay() {
        val p = AppPrefs(quietHours = true, quietFrom = 9 * 60, quietTo = 17 * 60)
        assertTrue(inQuietHours(p, 9 * 60))
        assertTrue(inQuietHours(p, 12 * 60))
        assertFalse(inQuietHours(p, 17 * 60))
        assertFalse(inQuietHours(p, 8 * 60))
    }

    @Test
    fun quietHoursOverMidnight() {
        val p = AppPrefs(quietHours = true, quietFrom = 22 * 60, quietTo = 8 * 60)
        assertTrue(inQuietHours(p, 23 * 60))
        assertTrue(inQuietHours(p, 0))
        assertTrue(inQuietHours(p, 7 * 60 + 59))
        assertFalse(inQuietHours(p, 8 * 60))
        assertFalse(inQuietHours(p, 12 * 60))
    }

    @Test
    fun quietHoursOffOrEmptyNeverApply() {
        assertFalse(inQuietHours(AppPrefs(quietHours = false), 23 * 60))
        assertFalse(inQuietHours(AppPrefs(quietHours = true, quietFrom = 600, quietTo = 600), 600))
    }

    @Test
    fun timesFormatForBothClocks() {
        assertEquals("22:00", formatMinute(22 * 60, true))
        assertEquals("10:00 PM", formatMinute(22 * 60, false))
        assertEquals("12:30 AM", formatMinute(30, false))
        assertEquals("12:00 PM", formatMinute(12 * 60, false))
        assertEquals("08:30", formatMinute(8 * 60 + 30, true))
    }

    @Test
    fun theStoreKeepsValuesInRangeAndResets() {
        val store = object : MemorySettingsStore() { override fun persist(prefs: AppPrefs) {} }
        store.update { it.copy(quietFrom = 5000, idleMinutes = 3, signInStatus = "y".repeat(300)) }
        assertEquals(24 * 60 - 1, store.prefs.value.quietFrom)
        assertEquals(5, store.prefs.value.idleMinutes)
        assertEquals(128, store.prefs.value.signInStatus.length)
        store.update { it.copy(theme = ThemeMode.Light, showPresence = false) }
        assertEquals(ThemeMode.Light, store.themeMode.value)
        assertFalse(store.showPresence.value)
        store.reset()
        assertEquals(AppPrefs(), store.prefs.value)
        assertTrue(store.showPresence.value)
    }
}
