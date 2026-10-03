package space.foid.chord.ui.spaces

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.viewmodel.contact
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.NotificationSetting

class SpaceLogicTest {
    @Test
    fun slugMatchesTheDesktop() {
        assertEquals("launch-ops", channelSlug("  Launch Ops  "))
        assertEquals("a-b", channelSlug("a!!!b"))
        assertEquals("general", channelSlug("--General--"))
        assertEquals("", channelSlug("   !!! "))
        assertEquals("caf", channelSlug("Café"))
    }

    @Test
    fun roomServiceComesFromAKnownRoomElseFromThePubsubService() {
        assertEquals("conference.chat.foid.space", roomServiceFor("pubsub.chat.foid.space", listOf("a@conference.chat.foid.space")))
        assertEquals("rooms.other.org", roomServiceFor("pubsub.chat.foid.space", listOf("a@rooms.other.org/nick")))
        assertEquals("conference.chat.foid.space", roomServiceFor("pubsub.chat.foid.space", emptyList()))
        assertNull(roomServiceFor("localhost", emptyList()))
    }

    @Test
    fun newRoomAddress() {
        assertEquals("club-general@conference.example.org", newRoomJid("club", "general", "conference.example.org"))
    }

    @Test
    fun nickFallsBackToTheAccountName() {
        assertEquals("Abby", spaceNick("  Abby ", "me@example.org"))
        assertEquals("me", spaceNick(null, "me@example.org/phone"))
        assertEquals("me", spaceNick("  ", "me@example.org"))
    }

    @Test
    fun inviteLinkAndText() {
        assertEquals("xmpp:pubsub.example.org?;node=club", spaceInviteLink("pubsub.example.org", "club"))
        assertEquals("xmpp:pubsub.example.org?;node=my%20club%2F1", spaceInviteLink("pubsub.example.org", "my club/1"))
        assertEquals("Join Club on Chord: xmpp:x?;node=y", inviteText("Club", "xmpp:x?;node=y"))
    }

    @Test
    fun muteDurations() {
        assertEquals(1_000L + 15 * 60_000, muteUntil(MuteDuration.Minutes15, 1_000))
        assertEquals(60 * 60_000L, muteUntil(MuteDuration.Hour1, 0))
        assertEquals(8 * 3_600_000L, muteUntil(MuteDuration.Hours8, 0))
        assertEquals(24 * 3_600_000L, muteUntil(MuteDuration.Hours24, 0))
        assertNull(muteUntil(MuteDuration.Forever, 5))
        assertEquals(5, MuteDuration.entries.size)
    }

    @Test
    fun muteStatusReadsTheLevelAndTheEnd() {
        assertEquals(MuteStatus.Off, muteStatus(null, 0))
        assertEquals(MuteStatus.Off, muteStatus(NotificationSetting(NotificationLevel.ALL, null), 0))
        assertEquals(MuteStatus.Forever, muteStatus(NotificationSetting(NotificationLevel.NONE, null), 0))
        assertEquals(MuteStatus.Until(500), muteStatus(NotificationSetting(NotificationLevel.MENTIONS, 500), 100))
        // A mute that ended is off.
        assertEquals(MuteStatus.Off, muteStatus(NotificationSetting(NotificationLevel.ALL, 500), 500))
        assertEquals(MuteStatus.Off, muteStatus(NotificationSetting(NotificationLevel.ALL, 400), 500))
    }

    @Test
    fun spaceLevelIsTheSharedLevelElseAll() {
        assertEquals(NotificationLevel.MENTIONS, spaceLevel(listOf(NotificationLevel.MENTIONS, NotificationLevel.MENTIONS)))
        assertEquals(NotificationLevel.ALL, spaceLevel(listOf(NotificationLevel.MENTIONS, NotificationLevel.NONE)))
        assertEquals(NotificationLevel.ALL, spaceLevel(emptyList()))
        assertEquals(NotificationLevel.NONE, spaceLevel(listOf(NotificationLevel.NONE)))
    }

    @Test
    fun contactFilterMatchesNameAndAddress() {
        val list = listOf(contact("rin@example.org", "Rin"), contact("sam@example.org"), contact("jo@other.net", "Josephine"))
        assertEquals(3, filterContacts(list, "  ").size)
        assertEquals(listOf("rin@example.org"), filterContacts(list, "RIN").map { it.jid })
        assertEquals(listOf("jo@other.net"), filterContacts(list, "other").map { it.jid })
        assertTrue(filterContacts(list, "zzz").isEmpty())
        assertEquals("sam", list[1].shownName())
        assertEquals("Rin", list[0].shownName())
    }

    @Test
    fun affiliationLabels() {
        assertEquals("Owner", affiliationLabel("owner"))
        assertEquals("Publisher", affiliationLabel("publish-only"))
        assertEquals("Banned", affiliationLabel("outcast"))
        assertEquals("custom", affiliationLabel("custom"))
    }

    @Test
    fun settingsChangeSendsOnlyWhatChanged() {
        assertTrue(settingsChange("Club", "About", "Club", "About").isEmpty)
        assertTrue(settingsChange("Club", "About", "  Club ", " About ").isEmpty)
        assertEquals(SettingsChange("New", null), settingsChange("Club", "About", "New", "About"))
        assertEquals(SettingsChange(null, "Fresh"), settingsChange("Club", "About", "Club", "Fresh"))
        // A blank name is no change. A blank description clears it.
        assertEquals(SettingsChange(null, ""), settingsChange("Club", "About", "  ", ""))
    }
}
