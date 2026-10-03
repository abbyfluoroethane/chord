package space.foid.chord.ui.profile

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
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
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.screens.DrawerFixtures
import space.foid.chord.ui.screens.MemberDrawerContent
import space.foid.chord.ui.sheets.SheetFrame
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.viewmodel.ProfileNotice
import space.foid.chord.viewmodel.ProfileState
import uniffi.chord_ffi.SpaceItem

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h780dp-xxhdpi")
class ProfileScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val spaces = listOf(
        SpaceItem("muc.chord.localhost", "design", "Design Guild", null),
        SpaceItem("muc.chord.localhost", "rust", "Rustaceans", null),
    )
    private val self = ProfileState(
        "me@chord.localhost", "Abby", presence = Presence.Online, status = "Prepping the static fire",
        isMe = true, affiliation = "owner", role = "moderator", sharedSpaces = spaces,
    )
    private val contact = ProfileState(
        "rin@chord.localhost", "Rin", presence = Presence.Away, status = "On the range until 17:00",
        isContact = true, affiliation = "admin", role = "moderator", fullName = "Rin Okafor", sharedSpaces = spaces,
    )
    private val stranger = ProfileState(
        "sam@chord.localhost", "Sam", presence = Presence.Offline, sharedSpaces = emptyList(), invitable = spaces,
    )
    private val occupant = ProfileState(
        "lounge@muc.chord.localhost/Zed", "Zed", presence = Presence.Online, hidden = true, role = "visitor",
    )
    private val blocked = stranger.copy(isBlocked = true)

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { content() } }
        compose.onRoot().captureRoboImage("src/test/screenshots/profile/$name.png")
    }

    @Composable private fun Sheet(s: ProfileState, notice: ProfileNotice? = null) = SheetFrame {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState())) {
            ProfileContent(s, ProfileVariant.Sheet, ProfileCallbacks(), Chord.colors.surface200, notice = notice)
        }
    }

    @Composable private fun Rail(s: ProfileState) = Column(Modifier.fillMaxWidth().background(Chord.colors.surfaceSide)) {
        ProfileContent(s, ProfileVariant.Rail, ProfileCallbacks(onViewFull = {}), Chord.colors.surfaceSide)
    }

    @Test fun self_dark() = shot(true, "self_dark") { Sheet(self) }
    @Test fun self_light() = shot(false, "self_light") { Sheet(self) }
    @Test fun contact_dark() = shot(true, "contact_dark") { Sheet(contact, ProfileNotice.Renamed) }
    @Test fun contact_light() = shot(false, "contact_light") { Sheet(contact) }
    @Test fun stranger_dark() = shot(true, "stranger_dark") { Sheet(stranger) }
    @Test fun stranger_light() = shot(false, "stranger_light") { Sheet(stranger) }
    @Test fun blocked_dark() = shot(true, "blocked_dark") { Sheet(blocked) }
    @Test fun occupant_dark() = shot(true, "occupant_dark") { Sheet(occupant) }
    @Test fun occupant_light() = shot(false, "occupant_light") { Sheet(occupant) }
    @Test fun rail_dark() = shot(true, "rail_dark") { Rail(contact) }
    @Test fun rail_light() = shot(false, "rail_light") { Rail(contact) }

    @Test fun members_dark() = shot(true, "members_dark") { MemberDrawerContent("general", DrawerFixtures.members, loaded = true) }
    @Test fun members_light() = shot(false, "members_light") { MemberDrawerContent("general", DrawerFixtures.members, loaded = true) }

    @Test fun menu_dark() = shot(true, "menu_dark") {
        SheetFrame { PersonMenuItems(stranger, true, {}, {}, {}, {}, {}, {}) }
    }
    @Test fun menu_light() = shot(false, "menu_light") {
        SheetFrame { PersonMenuItems(contact.copy(invitable = spaces), true, {}, {}, {}, {}, {}, {}) }
    }
}
