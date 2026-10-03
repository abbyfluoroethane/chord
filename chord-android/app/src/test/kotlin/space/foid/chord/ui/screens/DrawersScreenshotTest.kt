package space.foid.chord.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.layout.DrawerPane
import space.foid.chord.ui.layout.DualDrawer
import space.foid.chord.ui.layout.DualDrawerState
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.MemberItem
import uniffi.chord_ffi.SpaceItem

internal object DrawerFixtures {
    val spaces = listOf(
        SpaceItem("muc.chord.localhost", "design", "Design Guild", null),
        SpaceItem("muc.chord.localhost", "rust", "Rustaceans", null),
        SpaceItem("muc.chord.localhost", "gaming", "Game Night", null),
    )
    val spaceUnread = mapOf("muc.chord.localhost|rust" to 3, "muc.chord.localhost|gaming" to 120)

    private fun room(name: String, unread: Int = 0, category: String? = null) = ChannelItem(
        jid = "$name@muc.chord.localhost", name = name, kind = ChannelKind.Room, category = category,
        joined = true, lastActivity = null, unread = unread.toUInt(), blocked = false, members = null,
    )

    private fun dm(name: String, unread: Int = 0) = ChannelItem(
        jid = "${name.lowercase()}@chord.localhost", name = name, kind = ChannelKind.Direct, category = null,
        joined = true, lastActivity = null, unread = unread.toUInt(), blocked = false, members = null,
    )

    val homeChannels = listOf(
        dm("Alice Martin", unread = 2),
        dm("Bob"),
        dm("Carol Nguyen", unread = 11),
        ChannelItem(
            "lounge@muc.chord.localhost", "Lounge", ChannelKind.Room, null, true, null, 0u, false, 6u,
        ),
    )
    val spaceChannels = listOf(
        room("general", unread = 4),
        room("announcements"),
        room("design-review", unread = 1, category = "Work"),
        room("typography", category = "Work"),
        room("off-topic", category = "Social"),
    )

    fun member(
        id: String, name: String, online: Boolean = true, affiliation: String = "none",
        role: String = "participant", show: String? = null, status: String? = null,
    ) = MemberItem(id, name, "$id@chord.localhost", role, affiliation, show, status, online, null)

    val members = listOf(
        member("alice", "Alice Martin", affiliation = "owner", role = "moderator", status = "Reading the spec"),
        member("bob", "Bob", affiliation = "admin", role = "moderator", show = "away"),
        member("carol", "Carol Nguyen"),
        member("dave", "Dave", show = "dnd", status = "In a meeting"),
        member("erin", "Erin", role = "visitor"),
        member("frank", "Frank", online = false),
        member("grace", "Grace Hopper", online = false),
    )

    val account = AccountUi("me@chord.localhost", "Me")
}

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h780dp-xxhdpi")
class DrawersScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { content() } }
        compose.onRoot().captureRoboImage("src/test/screenshots/screens/$name.png")
    }

    @Composable private fun Home() = ChannelDrawerContent(
        spaces = DrawerFixtures.spaces, scope = ChannelScope.Home, onScope = {},
        channels = DrawerFixtures.homeChannels, loaded = true, selectedJid = "bob@chord.localhost",
        onSelect = {}, account = DrawerFixtures.account, onSignOut = {},
        spaceUnread = DrawerFixtures.spaceUnread, homeUnread = 13,
    )

    @Composable private fun Space() = ChannelDrawerContent(
        spaces = DrawerFixtures.spaces, scope = ChannelScope.Space("muc.chord.localhost", "design"), onScope = {},
        channels = DrawerFixtures.spaceChannels, loaded = true, selectedJid = "general@muc.chord.localhost",
        onSelect = {}, account = DrawerFixtures.account, onSignOut = {},
        spaceUnread = DrawerFixtures.spaceUnread, homeUnread = 13,
    )

    @Composable private fun Members() = MemberDrawerContent("general", DrawerFixtures.members, loaded = true)

    @Composable private fun Center() {
        Box(Modifier.fillMaxSize().background(Chord.colors.surface100), contentAlignment = Alignment.Center) {
            Text("Timeline", color = Chord.colors.ink)
        }
    }

    /** The container with the pane [fraction] of the way open (1 is fully open). */
    @Composable private fun Main(pane: DrawerPane, fraction: Float) {
        val state = remember { DualDrawerState(pane) }
        DualDrawer(state, left = { Home() }, right = { Members() }) { Center() }
        // The first layout snaps to the pane. Then the drag moves it back to [fraction].
        androidx.compose.runtime.LaunchedEffect(Unit) {
            androidx.compose.runtime.withFrameNanos { }
            if (fraction < 1f) {
                val width = state.offset
                state.draggable.dispatchRawDelta(-(width * (1f - fraction)))
            }
        }
    }

    @Test fun channel_drawer_home_dark() = shot(true, "channel_drawer_home_dark") { Home() }
    @Test fun channel_drawer_home_light() = shot(false, "channel_drawer_home_light") { Home() }
    @Test fun channel_drawer_space_dark() = shot(true, "channel_drawer_space_dark") { Space() }
    @Test fun channel_drawer_space_light() = shot(false, "channel_drawer_space_light") { Space() }
    @Test fun member_drawer_dark() = shot(true, "member_drawer_dark") { Members() }
    @Test fun member_drawer_light() = shot(false, "member_drawer_light") { Members() }
    @Test fun main_left_open_dark() = shot(true, "main_left_open_dark") { Main(DrawerPane.Left, 1f) }
    @Test fun main_left_open_light() = shot(false, "main_left_open_light") { Main(DrawerPane.Left, 1f) }
    @Test fun main_left_half_dark() = shot(true, "main_left_half_dark") { Main(DrawerPane.Left, 0.5f) }
    @Test fun main_left_half_light() = shot(false, "main_left_half_light") { Main(DrawerPane.Left, 0.5f) }
    @Test fun main_right_open_dark() = shot(true, "main_right_open_dark") { Main(DrawerPane.Right, 1f) }
    @Test fun main_right_open_light() = shot(false, "main_right_open_light") { Main(DrawerPane.Right, 1f) }
    @Test fun main_right_half_dark() = shot(true, "main_right_half_dark") { Main(DrawerPane.Right, 0.5f) }
    @Test fun main_right_half_light() = shot(false, "main_right_half_light") { Main(DrawerPane.Right, 0.5f) }
}
