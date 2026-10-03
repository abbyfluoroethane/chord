package space.foid.chord.ui.home

import androidx.compose.runtime.Composable
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.contacts.ContactsFixtures
import space.foid.chord.ui.screens.ChannelDrawerContent
import space.foid.chord.ui.screens.DrawerFixtures
import space.foid.chord.ui.theme.ChordTheme
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.ChannelScope

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h780dp-xxhdpi")
class HomeScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { content() } }
        compose.onRoot().captureRoboImage("src/test/screenshots/home/$name.png")
    }

    private fun dm(name: String, unread: Int = 0, jid: String = "${name.lowercase()}@chord.localhost") = ChannelItem(
        jid, name, ChannelKind.Direct, null, true, null, unread.toUInt(), false, null,
    )

    private val chats = listOf(
        dm("Alice Martin", unread = 2, jid = "alice@chord.localhost"),
        ChannelItem("padcrew@conference.chord.localhost", "Pad Crew", ChannelKind.Room, null, true, null, 3u, false, 4u),
        dm("Bob", jid = "bob@chord.localhost"),
        dm("Carol Nguyen", jid = "carol@chord.localhost"),
        dm("Dave", jid = "dave@chord.localhost"),
        ChannelItem("lounge@conference.chord.localhost", "Lounge", ChannelKind.Room, null, true, null, 0u, false, 1u),
    )

    @Composable private fun Drawer(channels: List<ChannelItem>, pending: Int, selected: String? = "bob@chord.localhost") = ChannelDrawerContent(
        spaces = DrawerFixtures.spaces, scope = ChannelScope.Home, onScope = {},
        channels = channels, loaded = true, selectedJid = selected,
        onSelect = {}, account = DrawerFixtures.account, onSignOut = {},
        spaceUnread = DrawerFixtures.spaceUnread, homeUnread = 5,
        contacts = ContactsFixtures.contacts, pendingContacts = pending,
    )

    @Test fun home_list_dark() = shot(true, "home_list_dark") { Drawer(chats, pending = 2) }
    @Test fun home_list_light() = shot(false, "home_list_light") { Drawer(chats, pending = 2) }
    @Test fun home_empty_dark() = shot(true, "home_empty_dark") { Drawer(emptyList(), pending = 0, selected = null) }
    @Test fun home_empty_light() = shot(false, "home_empty_light") { Drawer(emptyList(), pending = 0, selected = null) }

    // ---- Find or start a chat ----
    private val spaces = DrawerFixtures.spaces
    private fun hits(q: String) = findHits(q, chats, DrawerFixtures.spaceChannels, ContactsFixtures.contacts, spaces)

    @Composable private fun Find(q: String) = FindChatContent(
        query = q, onQuery = {}, hits = hits(q), contacts = ContactsFixtures.contacts, onPick = {}, onBack = {},
    )

    @Test fun find_empty_dark() = shot(true, "find_empty_dark") { Find("") }
    @Test fun find_empty_light() = shot(false, "find_empty_light") { Find("") }
    @Test fun find_query_dark() = shot(true, "find_query_dark") { Find("al") }
    @Test fun find_query_light() = shot(false, "find_query_light") { Find("al") }
    @Test fun find_address_dark() = shot(true, "find_address_dark") { Find("sam@chord.example") }
    @Test fun find_address_light() = shot(false, "find_address_light") { Find("sam@chord.example") }
    @Test fun find_no_match_dark() = shot(true, "find_no_match_dark") { Find("zzzz") }
}
