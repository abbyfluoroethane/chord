package space.foid.chord.ui.home

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.ui.contacts.ContactsFixtures
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.SpaceItem

class FindLogicTest {
    private fun dm(name: String, jid: String = "${name.lowercase()}@chord.localhost") =
        ChannelItem(jid, name, ChannelKind.Direct, null, true, null, 0u, false, null)

    private fun group(name: String) =
        ChannelItem("${name.lowercase()}@conference.chord.localhost", name, ChannelKind.Room, null, true, null, 0u, false, 4u)

    private fun room(name: String) =
        ChannelItem("$name@muc.chord.localhost", name, ChannelKind.Room, null, true, null, 0u, false, null)

    private val home = listOf(dm("Alice"), group("Pad Crew"), dm("Bob"))
    private val spaces = listOf(SpaceItem("muc.chord.localhost", "design", "Design Guild", null))
    private val contacts = ContactsFixtures.contacts

    private fun titles(q: String, space: List<ChannelItem> = emptyList()) =
        findHits(q, home, space, contacts, spaces).map { it.title }

    @Test
    fun fuzzyScoreNeedsTheLettersInOrder() {
        assertEquals(0.0, fuzzyScore("zq", "Alice"), 0.0)
        assertEquals(0.0, fuzzyScore("ecila", "Alice"), 0.0)
        assertTrue(fuzzyScore("ali", "Alice") > 0)
        assertEquals(1.0, fuzzyScore("", "anything"), 0.0)
    }

    @Test
    fun prefixAndWordStartScoreHigher() {
        assertTrue(fuzzyScore("pad", "Pad Crew") > fuzzyScore("pad", "Spade"))
        assertTrue(fuzzyScore("cr", "Pad Crew") > fuzzyScore("cr", "Scrap"))
    }

    @Test
    fun emptyQueryListsChatsThenContactsWithoutAChatThenSpaces() {
        val t = titles("")
        assertEquals(listOf("Alice", "Pad Crew", "Bob"), t.take(3))
        // Alice and Bob have a chat. Their contact entries are not repeated by address.
        assertTrue("Dave" in t)
        assertEquals("Design Guild", t.last())
    }

    @Test
    fun theQueryFiltersAndSorts() {
        assertEquals("Pad Crew", titles("pad").first())
        assertTrue(titles("qqq").isEmpty())
    }

    @Test
    fun hashKeepsGroupChatsAndRoomsOfTheSpace() {
        val t = titles("#", space = listOf(room("general")))
        assertEquals(listOf("Pad Crew", "general"), t)
    }

    @Test
    fun atKeepsPeople() {
        val t = titles("@")
        assertTrue("Alice" in t && "Dave" in t)
        assertTrue("Pad Crew" !in t && "Design Guild" !in t)
    }

    @Test
    fun aTypedAddressGivesMessageAndJoinFirst() {
        val hits = findHits("new@other.example", home, emptyList(), contacts, spaces)
        assertEquals(listOf(FindHit.MessageAddress("new@other.example"), FindHit.JoinAddress("new@other.example")), hits)
    }

    @Test
    fun aKnownAddressGivesNoTypedHit() {
        val hits = findHits("alice@chord.localhost", home, emptyList(), contacts, spaces)
        assertTrue(hits.none { it is FindHit.MessageAddress || it is FindHit.JoinAddress })
    }

    @Test
    fun anAddressNeedsADotInTheDomain() {
        assertTrue(typedHits("sam@localhost", emptySet()).isEmpty())
        assertTrue(typedHits("not an address", emptySet()).isEmpty())
        assertEquals(2, typedHits("@sam@chord.example", emptySet()).size)
    }

    @Test
    fun anXmppLinkGivesALinkHit() {
        val hits = typedHits("xmpp:room@conference.example.org?join", emptySet())
        assertEquals(FindHit.Link("xmpp:room@conference.example.org?join"), hits.single())
    }

    @Test
    fun theListHasALimit() {
        val many = (1..60).map { dm("User$it") }
        assertEquals(FIND_LIMIT, findHits("", many, emptyList(), emptyList(), emptyList()).size)
    }

    @Test
    fun groupMemberCountShowsOnlyForRooms() {
        assertEquals(4, groupMemberCount(group("Pad Crew")))
        assertEquals(null, groupMemberCount(dm("Alice")))
    }
}
