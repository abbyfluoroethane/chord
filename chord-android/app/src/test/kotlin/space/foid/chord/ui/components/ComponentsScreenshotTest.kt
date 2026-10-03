package space.foid.chord.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordTheme

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h640dp-xxhdpi")
class ComponentsScreenshotTest {
    @get:Rule val compose = createComposeRule()

    @Test fun avatar_dark() = shot(true, "avatar_dark") { AvatarSheet() }
    @Test fun avatar_light() = shot(false, "avatar_light") { AvatarSheet() }
    @Test fun space_rail_dark() = shot(true, "space_rail_dark") { RailSheet() }
    @Test fun space_rail_light() = shot(false, "space_rail_light") { RailSheet() }
    @Test fun channel_list_dark() = shot(true, "channel_list_dark") { ChannelSheet() }
    @Test fun channel_list_light() = shot(false, "channel_list_light") { ChannelSheet() }

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { content() } }
        compose.onRoot().captureRoboImage("src/test/screenshots/components/$name.png")
    }

    @Composable
    private fun AvatarSheet() {
        Column(
            Modifier.background(Chord.colors.surface100).padding(ChordSpace.s4),
            verticalArrangement = Arrangement.spacedBy(ChordSpace.s4),
        ) {
            val jids = listOf("alice@example.org", "bob@example.org", "carol@example.org", "dave@example.org", "eve@example.org")
            Row(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s4)) {
                jids.forEach { Avatar(jid = it, name = it.substringBefore('@').replaceFirstChar(Char::uppercase)) }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s6)) {
                Avatar("alice@example.org", name = "Alice Smith", presence = Presence.Online)
                Avatar("bob@example.org", name = "Bob", presence = Presence.Away)
                Avatar("carol@example.org", name = "Carol", presence = Presence.Dnd)
                Avatar("dave@example.org", name = "Dave", presence = Presence.Offline)
                Avatar("eve@example.org", name = "Eve")
            }
            Row(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s6), verticalAlignment = androidx.compose.ui.Alignment.Bottom) {
                Avatar("alice@example.org", name = "Alice Smith", size = 24.dp, presence = Presence.Online)
                Avatar("alice@example.org", name = "Alice Smith", size = 32.dp, presence = Presence.Away)
                Avatar("alice@example.org", name = "Alice Smith", size = 56.dp, presence = Presence.Dnd)
                Avatar("alice@example.org", name = "Alice Smith", size = 80.dp, presence = Presence.Online)
            }
        }
    }

    @Composable
    private fun RailSheet() {
        val c = Chord.colors
        Row(
            Modifier.background(c.surfaceRail).padding(vertical = ChordSpace.s2),
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s4),
        ) {
            Column {
                SpaceRailIcon("Home", selected = false, onClick = {}, kind = RailIconKind.Home)
                SpaceRailIcon("Home", selected = true, onClick = {}, kind = RailIconKind.Home)
                SpaceRailIcon("Foid Lab", selected = false, onClick = {})
                SpaceRailIcon("Foid Lab", selected = true, onClick = {})
                SpaceRailIcon("Garden Club", selected = false, unread = 3, onClick = {})
                SpaceRailIcon("Rust Users", selected = false, unread = 5, mentions = 2, onClick = {})
                SpaceRailIcon("Big Space", selected = false, mentions = 120, onClick = {})
                SpaceRailIcon("Chess", selected = true, mentions = 1, onClick = {})
            }
            Column {
                SpaceRailIcon("Home", selected = false, mentions = 4, onClick = {}, kind = RailIconKind.Home)
                SpaceRailIcon("Pixel Art", selected = false, onClick = {})
                SpaceRailIcon("Zeta", selected = false, onClick = {})
                SpaceRailIcon("Yellow Cafe", selected = false, onClick = {})
            }
        }
    }

    @Composable
    private fun ChannelSheet() {
        val c = Chord.colors
        Column(Modifier.background(c.surfaceSide).padding(vertical = ChordSpace.s2)) {
            ChannelListItem("general", selected = false, onClick = {})
            ChannelListItem("announcements", selected = false, unread = 4, onClick = {})
            ChannelListItem("dev", selected = false, unread = 9, mentions = 3, onClick = {})
            ChannelListItem("random", selected = true, onClick = {})
            ChannelListItem("noisy", selected = false, unread = 20, muted = true, onClick = {})
            ChannelListItem("a-very-long-channel-name-that-needs-to-be-cut-off-at-the-end", selected = false, unread = 1, onClick = {})
            ChannelListItem("Alice Smith", jid = "alice@example.org", kind = ChannelKind.Dm, presence = Presence.Online, selected = false, onClick = {})
            ChannelListItem("Bob", jid = "bob@example.org", kind = ChannelKind.Dm, presence = Presence.Away, unread = 2, selected = false, onClick = {})
            ChannelListItem("Carol", jid = "carol@example.org", kind = ChannelKind.Dm, presence = Presence.Dnd, selected = true, onClick = {})
            ChannelListItem("Dave", jid = "dave@example.org", kind = ChannelKind.Dm, presence = Presence.Offline, selected = false, onClick = {})
            ChannelListItem("Eve", jid = "eve@example.org", kind = ChannelKind.Dm, presence = Presence.Online, muted = true, unread = 3, selected = false, onClick = {})
            ChannelListItem("Book club", jid = "book@example.org", kind = ChannelKind.Group, subtitle = "Alice, Bob, Carol and 2 more", unread = 6, selected = false, onClick = {})
        }
    }

}
