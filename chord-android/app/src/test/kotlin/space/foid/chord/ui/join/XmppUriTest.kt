package space.foid.chord.ui.join

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class XmppUriTest {
    @Test
    fun bareAddressIsLowerCasedAndLosesItsResource() {
        assertEquals("room@conference.example.org", parseBareJid("  Room@Conference.Example.org/phone "))
    }

    @Test
    fun badAddressesAreRejected() {
        listOf("", "room", "@host.org", "a@b@c.org", "a b@host.org", "a@", "a@host..org", "a@-host.org", "a/b@host.org")
            .forEach { assertNull("'$it'", parseBareJid(it)) }
    }

    @Test
    fun joinLinkGivesARoomWithItsPassword() {
        assertEquals(
            XmppTarget.Room("chord-smoke@conference.chat.foid.space", "p w"),
            parseXmppUri("xmpp:chord-smoke@conference.chat.foid.space?join;password=p%20w"),
        )
        assertEquals(XmppTarget.Room("a@b.org", null), parseXmppUri("XMPP:a@b.org?JOIN"))
    }

    @Test
    fun plainAddressIsAChat() {
        assertEquals(XmppTarget.Chat("bob@example.org"), parseXmppUri("xmpp:bob@example.org"))
        assertEquals(XmppTarget.Chat("bob@example.org", "hi"), parseXmppUri("xmpp:bob@example.org?message;body=hi"))
    }

    @Test
    fun rosterLinkGivesAContactWithAName() {
        assertEquals(XmppTarget.Contact("bob@example.org", "Bob"), parseXmppUri("xmpp:bob@example.org?roster;name=Bob"))
        assertEquals(XmppTarget.Contact("bob@example.org", null), parseXmppUri("xmpp:bob@example.org?subscribe"))
    }

    @Test
    fun spaceLinkInBothForms() {
        assertEquals(XmppTarget.Space("pubsub.example.org", "design"), parseXmppUri("xmpp:pubsub.example.org?;node=design"))
        assertEquals(
            XmppTarget.Space("pubsub.example.org", "design"),
            parseXmppUri("xmpp:pubsub.example.org?pubsub;action=subscribe;node=design"),
        )
        assertEquals(XmppTarget.Invalid, parseXmppUri("xmpp:pubsub.example.org?pubsub;action=publish;node=design"))
    }

    @Test
    fun badLinksAreInvalid() {
        listOf(
            "", "http://example.org", "xmpp:", "xmpp://other@example.org/bob@example.org", "xmpp:bob@example.org?unknown",
            "xmpp:bob@example.org?sendfile", "xmpp:bob example.org", "xmpp:bad@@example.org?join", "xmpp:%zz@example.org",
            "xmpp:bob@example.org?join;password=%zz", "xmpp:" + "a".repeat(MAX_URI_LENGTH) + "@example.org",
        ).forEach { assertEquals("'$it'", XmppTarget.Invalid, parseXmppUri(it)) }
    }

    @Test
    fun fragmentIsIgnored() {
        assertEquals(XmppTarget.Room("a@b.org", null), parseXmppUri("xmpp:a@b.org?join#top"))
    }

    @Test
    fun roomFieldAcceptsAddressOrUri() {
        assertEquals(XmppTarget.Room("a@b.org"), roomTargetOf("a@b.org"))
        assertEquals(XmppTarget.Room("a@b.org", "x"), roomTargetOf("xmpp:a@b.org?join;password=x"))
        assertEquals(XmppTarget.Room("a@b.org"), roomTargetOf("xmpp:a@b.org"))
        assertNull(roomTargetOf("xmpp:a@b.org?roster"))
        assertNull(roomTargetOf("nonsense"))
    }

    @Test
    fun personFieldAcceptsAddressOrUri() {
        assertEquals("bob@b.org", personAddressOf("Bob@b.org"))
        assertEquals("bob@b.org", personAddressOf("xmpp:bob@b.org?message"))
        assertNull(personAddressOf("bob"))
        assertNull(personAddressOf("xmpp:b.org?;node=x"))
    }
}
