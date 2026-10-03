package space.foid.chord.ui.contacts

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.contacts.ContactsFixtures.blocked
import space.foid.chord.ui.contacts.ContactsFixtures.contact
import space.foid.chord.ui.contacts.ContactsFixtures.contacts
import space.foid.chord.viewmodel.RequestItem
import uniffi.chord_ffi.SubscriptionState

class ContactsLogicTest {
    private fun names(tab: ContactsTab, query: String = "", requests: List<RequestItem> = emptyList()) =
        contactEntries(tab, contacts, blocked, requests, query).map { it.name }

    @Test
    fun nameFallsBackToTheLocalPart() {
        assertEquals("Alice Martin", contactName(contact("alice@chord.localhost", "Alice Martin")))
        assertEquals("erin", contactName(contact("erin@other.example", null)))
        assertEquals("erin", contactName(contact("erin@other.example", "  ")))
    }

    @Test
    fun onlineTabListsOnlineContactsByName() {
        assertEquals(listOf("Alice Martin", "Bob", "Carol Nguyen", "erin"), names(ContactsTab.Online))
    }

    @Test
    fun allTabHidesBlockedAndOutgoing() {
        val all = names(ContactsTab.All)
        assertEquals(listOf("Alice Martin", "Bob", "Carol Nguyen", "Dave", "erin"), all)
        assertFalse("Spammer" in all)
        assertFalse("Mika" in all)
    }

    @Test
    fun outgoingNeedsAskAndNoSubscriptionFromThem() {
        assertTrue(contact("a@x.org", ask = true, subscription = SubscriptionState.NONE).isOutgoing())
        assertTrue(contact("a@x.org", ask = true, subscription = SubscriptionState.FROM).isOutgoing())
        assertFalse(contact("a@x.org", ask = true, subscription = SubscriptionState.TO).isOutgoing())
        assertFalse(contact("a@x.org", ask = false, subscription = SubscriptionState.NONE).isOutgoing())
    }

    @Test
    fun pendingListsIncomingBeforeOutgoing() {
        val rows = contactEntries(
            ContactsTab.Pending, contacts, blocked,
            listOf(RequestItem("zed@other.example"), RequestItem("amy@other.example")),
        )
        assertEquals(listOf(EntryKind.Incoming, EntryKind.Incoming, EntryKind.Outgoing), rows.map { it.kind })
        assertEquals(listOf("amy", "zed", "Mika"), rows.map { it.name })
    }

    @Test
    fun pendingUsesTheRosterNameOfARequester() {
        val rows = contactEntries(
            ContactsTab.Pending, listOf(contact("dave@chord.localhost", "Dave", subscription = SubscriptionState.TO)), emptyList(),
            listOf(RequestItem("dave@chord.localhost")),
        )
        assertEquals("Dave", rows.single().name)
    }

    @Test
    fun blockedTabHoldsTheBlocklistWithAndWithoutRosterEntry() {
        val rows = contactEntries(ContactsTab.Blocked, contacts, blocked, emptyList())
        assertEquals(listOf("Spammer", "stranger"), rows.map { it.name })
        assertTrue(rows.all { it.kind == EntryKind.Blocked })
    }

    @Test
    fun addTabHasNoRows() {
        assertTrue(names(ContactsTab.Add).isEmpty())
    }

    @Test
    fun searchMatchesNameAndAddressIgnoringCase() {
        assertEquals(listOf("Alice Martin"), names(ContactsTab.All, "ALICE"))
        assertEquals(listOf("erin"), names(ContactsTab.All, "other.example"))
        assertTrue(names(ContactsTab.All, "zzz").isEmpty())
        assertEquals(5, names(ContactsTab.All, "  ").size)
    }

    @Test
    fun presenceFollowsOnlineAndShow() {
        assertEquals(Presence.Offline, placeholderPresenceOf(false, "away"))
        assertEquals(Presence.Online, placeholderPresenceOf(true, null))
        assertEquals(Presence.Online, placeholderPresenceOf(true, "chat"))
        assertEquals(Presence.Away, placeholderPresenceOf(true, "away"))
        assertEquals(Presence.Away, placeholderPresenceOf(true, "xa"))
        assertEquals(Presence.Dnd, placeholderPresenceOf(true, "dnd"))
    }

    @Test
    fun statusTextIgnoresBlank() {
        assertEquals("Hello", contact("a@x.org", status = " Hello ").statusText())
        assertEquals(null, contact("a@x.org", status = "  ").statusText())
    }
}
