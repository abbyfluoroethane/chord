package space.foid.chord.service

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.chord_ffi.ConnectionState

class ServiceStatusTextTest {
    @Test
    fun textPerState() {
        assertEquals("Connected", ChordConnectionService.statusText(ConnectionState.Connected("a@b/r", false)))
        assertEquals("Connecting…", ChordConnectionService.statusText(ConnectionState.Connecting))
        assertEquals("Connecting…", ChordConnectionService.statusText(null))
        assertEquals("Offline, retrying", ChordConnectionService.statusText(ConnectionState.Suspended))
        assertEquals("Offline", ChordConnectionService.statusText(ConnectionState.Disconnected))
    }
}
