package space.foid.chord.notify

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChordNotificationsTest {
    @Test
    fun directChatIsDm() {
        assertEquals(ChordNotifications.CHANNEL_DM, ChordNotifications.channelFor(null, false))
        // A mention flag without a room still is a DM.
        assertEquals(ChordNotifications.CHANNEL_DM, ChordNotifications.channelFor(null, true))
    }

    @Test
    fun roomMentionIsMention() {
        assertEquals(ChordNotifications.CHANNEL_MENTION, ChordNotifications.channelFor("room@muc.example", true))
    }

    @Test
    fun otherRoomMessageIsOther() {
        assertEquals(ChordNotifications.CHANNEL_OTHER, ChordNotifications.channelFor("room@muc.example", false))
    }

    @Test
    fun onlyTheOpenChatIsSuppressed() {
        assertTrue(ChordNotifications.isSuppressed("a@b", "a@b"))
        assertFalse(ChordNotifications.isSuppressed("a@b", "c@d"))
        assertFalse(ChordNotifications.isSuppressed("a@b", null))
    }
}
