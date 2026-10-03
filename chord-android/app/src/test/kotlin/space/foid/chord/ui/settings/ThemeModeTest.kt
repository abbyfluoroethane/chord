package space.foid.chord.ui.settings

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.ui.components.ConnectionNotice
import space.foid.chord.ui.components.notice
import uniffi.chord_ffi.ConnectFailure
import uniffi.chord_ffi.ConnectionState

class ThemeModeTest {
    @Test
    fun systemFollowsTheSystem() {
        assertTrue(ThemeMode.System.isDark(true))
        assertFalse(ThemeMode.System.isDark(false))
    }

    @Test
    fun darkAndLightIgnoreTheSystem() {
        assertTrue(ThemeMode.Dark.isDark(false))
        assertFalse(ThemeMode.Light.isDark(true))
    }

    @Test
    fun unknownNamesFallBackToSystem() {
        assertEquals(ThemeMode.System, ThemeMode.fromName(null))
        assertEquals(ThemeMode.System, ThemeMode.fromName("Purple"))
        assertEquals(ThemeMode.Light, ThemeMode.fromName("Light"))
    }

    @Test
    fun theBannerFollowsTheConnection() {
        assertEquals(ConnectionNotice.Connecting, ConnectionState.Connecting.notice())
        assertEquals(ConnectionNotice.Offline, ConnectionState.Suspended.notice())
        assertEquals(ConnectionNotice.SignedOut, ConnectionState.AuthFailed("no").notice())
        assertEquals(null, ConnectionState.Connected("a@b.c/r", false).notice())
        assertEquals(null, ConnectionState.Disconnected.notice())
        assertEquals(null, ConnectionState.LoginFailed(ConnectFailure.Timeout).notice())
    }
}
