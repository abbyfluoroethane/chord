package space.foid.chord.ui.screens

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.ChannelScope

/** The rules are those of the desktop: ChannelSidebar.svelte and categories.ts. */
class ChannelSectionsTest {
    private fun room(name: String, category: String? = null, joined: Boolean = true) = ChannelItem(
        jid = "$name@muc.example.org", name = name, kind = ChannelKind.Room, category = category,
        joined = joined, lastActivity = null, unread = 0u, blocked = false, members = null,
    )

    private fun dm(name: String) = ChannelItem(
        jid = "$name@example.org", name = name, kind = ChannelKind.Direct, category = null,
        joined = true, lastActivity = null, unread = 0u, blocked = false, members = null,
    )

    private val space = ChannelScope.Space("muc.example.org", "design")

    @Test
    fun noChannelsGiveNoSection() {
        assertTrue(channelSections(ChannelScope.Home, emptyList()).isEmpty())
        assertTrue(channelSections(space, emptyList()).isEmpty())
    }

    @Test
    fun homeIsOneFlatListWithGroupChatsAmongTheDirectChats() {
        val items = listOf(dm("alice"), room("tinkerspace"), dm("bob"), room("jdev"))
        val sections = channelSections(ChannelScope.Home, items)
        assertEquals(1, sections.size)
        assertEquals("Messages", sections[0].title)
        // The order is kept: the core sorts by last activity.
        assertEquals(items, sections[0].items)
    }

    @Test
    fun homeIgnoresCategories() {
        val sections = channelSections(ChannelScope.Home, listOf(room("a", "Work")))
        assertEquals(listOf("Messages"), sections.map { it.title })
    }

    @Test
    fun aSpaceWithoutCategoriesHasOneChannelsSection() {
        val items = listOf(room("general"), room("random"))
        val sections = channelSections(space, items)
        assertEquals(listOf("Channels"), sections.map { it.title })
        assertEquals(items, sections[0].items)
    }

    @Test
    fun roomsWithoutCategoryComeFirstThenCategoriesInOrderOfFirstAppearance() {
        val sections = channelSections(
            space,
            listOf(
                room("design", "Work"), room("general"), room("games", "Fun"),
                room("review", "Work"), room("news", "  "), room("music", "Fun"),
            ),
        )
        assertEquals(listOf("Channels", "Work", "Fun"), sections.map { it.title })
        assertEquals(listOf("general", "news"), sections[0].items.map { it.name })
        assertEquals(listOf("design", "review"), sections[1].items.map { it.name })
        assertEquals(listOf("games", "music"), sections[2].items.map { it.name })
    }

    @Test
    fun theCategoryNameIsTrimmedForGrouping() {
        val sections = channelSections(space, listOf(room("a", "Work"), room("b", " Work ")))
        assertEquals(listOf("Channels", "Work"), sections.map { it.title })
        assertEquals(2, sections[1].items.size)
    }

    @Test
    fun theChannelsHeaderStaysWhenEveryRoomHasACategory() {
        val sections = channelSections(space, listOf(room("a", "Work")))
        assertEquals(listOf("Channels", "Work"), sections.map { it.title })
        assertTrue(sections[0].items.isEmpty())
    }

    @Test
    fun aRoomThatIsNotJoinedShowsLikeAnyOther() {
        val items = listOf(room("a"), room("b", joined = false))
        assertEquals(items, channelSections(space, items)[0].items)
    }

    @Test
    fun sectionKeysAreStableAndDistinct() {
        val sections = channelSections(space, listOf(room("a"), room("b", "Channels")))
        assertEquals(sections.size, sections.map { it.key }.toSet().size)
    }
}
