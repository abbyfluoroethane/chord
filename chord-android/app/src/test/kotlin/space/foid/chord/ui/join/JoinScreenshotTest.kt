package space.foid.chord.ui.join

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
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
import space.foid.chord.ui.screens.ChannelDrawerContent
import space.foid.chord.ui.screens.DrawerFixtures
import space.foid.chord.ui.sheets.SheetFrame
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.viewmodel.ActionKind
import space.foid.chord.viewmodel.ActionsState
import space.foid.chord.viewmodel.ChannelActionTarget
import space.foid.chord.viewmodel.JoinState
import space.foid.chord.viewmodel.JoinTab
import space.foid.chord.viewmodel.PasswordPrompt
import space.foid.chord.viewmodel.SpaceRow
import space.foid.chord.viewmodel.SpaceStatus
import space.foid.chord.viewmodel.SpacesState
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.SpaceInfo

// The sheet window is not drawn here: the tests draw the content in a frame styled like the sheet.
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h700dp-xxhdpi")
class JoinScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val room = "chord-smoke@conference.chat.foid.space"

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent {
            ChordTheme(dark = dark) {
                Box(Modifier.fillMaxSize().background(Chord.colors.surface100)) {
                    Box(Modifier.fillMaxSize().background(Chord.colors.scrim))
                    Box(Modifier.align(Alignment.BottomCenter)) { content() }
                }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/join/$name.png")
    }

    @Composable private fun New(state: JoinState) = SheetFrame { NewConversationContent(state, JoinCallbacks()) }

    @Composable private fun Actions(state: ActionsState) = SheetFrame {
        ChannelActionsContent(state, onMarkRead = {}, onLevel = {}, onCopy = {}, onLeave = {})
    }

    private val spaces = SpacesState.Loaded(
        listOf(
            SpaceRow(SpaceInfo("pubsub.chat.foid.space", "design", "Design Guild", "Type, colour and the occasional argument about kerning.", "open")),
            SpaceRow(SpaceInfo("pubsub.chat.foid.space", "rust", "Rustaceans", null, "open"), SpaceStatus.Joining),
            SpaceRow(SpaceInfo("pubsub.chat.foid.space", "club", "Closed Club", "Members only. The owner says yes or no.", "whitelist"), SpaceStatus.Requested),
            SpaceRow(
                SpaceInfo("pubsub.chat.foid.space", "games", "Game Night", "Friday evenings.", "open"),
                error = "The server refused the request.",
            ),
        ),
    )

    // ---- Join a room ----
    @Test fun room_empty_dark() = shot(true, "room_empty_dark") { New(JoinState(visible = true)) }
    @Test fun room_empty_light() = shot(false, "room_empty_light") { New(JoinState(visible = true)) }
    @Test fun room_filled_dark() = shot(true, "room_filled_dark") { New(JoinState(visible = true, address = "xmpp:$room?join", nick = "Abby")) }
    @Test fun room_joining_dark() = shot(true, "room_joining_dark") { New(JoinState(visible = true, address = room, joining = true)) }
    @Test fun room_joining_light() = shot(false, "room_joining_light") { New(JoinState(visible = true, address = room, joining = true)) }
    @Test fun room_members_only_dark() = shot(true, "room_members_only_dark") {
        New(JoinState(visible = true, address = room, roomError = "Only members can join this room. Ask an admin to add you."))
    }
    @Test fun room_members_only_light() = shot(false, "room_members_only_light") {
        New(JoinState(visible = true, address = room, roomError = "Only members can join this room. Ask an admin to add you."))
    }
    @Test fun room_banned_dark() = shot(true, "room_banned_dark") {
        New(JoinState(visible = true, address = room, roomError = "You are banned from this room."))
    }
    @Test fun room_bad_address_dark() = shot(true, "room_bad_address_dark") { New(JoinState(visible = true, address = "not an address")) }
    @Test fun room_password_dark() = shot(true, "room_password_dark") {
        New(JoinState(visible = true, address = room, passwordPrompt = PasswordPrompt(wrong = false)))
    }
    @Test fun room_password_light() = shot(false, "room_password_light") {
        New(JoinState(visible = true, address = room, passwordPrompt = PasswordPrompt(wrong = false), password = "hunter2"))
    }
    @Test fun room_password_wrong_dark() = shot(true, "room_password_wrong_dark") {
        New(JoinState(visible = true, address = room, passwordPrompt = PasswordPrompt(wrong = true), roomError = "That password was not accepted."))
    }

    // ---- Message someone ----
    @Test fun person_empty_dark() = shot(true, "person_empty_dark") { New(JoinState(visible = true, tab = JoinTab.Person)) }
    @Test fun person_empty_light() = shot(false, "person_empty_light") { New(JoinState(visible = true, tab = JoinTab.Person)) }
    @Test fun person_filled_dark() = shot(true, "person_filled_dark") {
        New(JoinState(visible = true, tab = JoinTab.Person, personJid = "chord-smoke2@chat.foid.space", personName = "Smoke Two"))
    }
    @Test fun person_error_dark() = shot(true, "person_error_dark") {
        New(JoinState(visible = true, tab = JoinTab.Person, personJid = "bob", personError = "That is not an address. Use name@server.example."))
    }
    @Test fun person_busy_dark() = shot(true, "person_busy_dark") {
        New(JoinState(visible = true, tab = JoinTab.Person, personJid = "bob@example.org", messaging = true))
    }

    // ---- Browse spaces ----
    @Test fun spaces_loading_dark() = shot(true, "spaces_loading_dark") { New(JoinState(visible = true, tab = JoinTab.Spaces, spaces = SpacesState.Loading)) }
    @Test fun spaces_list_dark() = shot(true, "spaces_list_dark") { New(JoinState(visible = true, tab = JoinTab.Spaces, spaces = spaces)) }
    @Test fun spaces_list_light() = shot(false, "spaces_list_light") { New(JoinState(visible = true, tab = JoinTab.Spaces, spaces = spaces)) }
    @Test fun spaces_empty_dark() = shot(true, "spaces_empty_dark") { New(JoinState(visible = true, tab = JoinTab.Spaces, spaces = SpacesState.Loaded(emptyList()))) }
    @Test fun spaces_failed_dark() = shot(true, "spaces_failed_dark") {
        New(JoinState(visible = true, tab = JoinTab.Spaces, spaces = SpacesState.Failed("Could not reach the server. Check your network and the server name.")))
    }

    // ---- Channel actions ----
    private val roomTarget = ChannelActionTarget(room, "chord-smoke", ActionKind.Room)
    private val contactTarget = ChannelActionTarget("alice@chord.localhost", "Alice Martin", ActionKind.Contact)

    @Test fun actions_room_dark() = shot(true, "actions_room_dark") { Actions(ActionsState(roomTarget, NotificationLevel.MENTIONS)) }
    @Test fun actions_room_light() = shot(false, "actions_room_light") { Actions(ActionsState(roomTarget, NotificationLevel.ALL)) }
    @Test fun actions_contact_dark() = shot(true, "actions_contact_dark") { Actions(ActionsState(contactTarget, NotificationLevel.NONE)) }
    @Test fun actions_contact_light() = shot(false, "actions_contact_light") { Actions(ActionsState(contactTarget, NotificationLevel.ALL)) }
    @Test fun actions_loading_dark() = shot(true, "actions_loading_dark") { Actions(ActionsState(roomTarget, null)) }
    @Test fun actions_error_dark() = shot(true, "actions_error_dark") {
        Actions(ActionsState(roomTarget, NotificationLevel.ALL, error = "You are not connected to the server."))
    }
    @Test fun actions_occupant_dark() = shot(true, "actions_occupant_dark") {
        Actions(ActionsState(ChannelActionTarget("$room/nick", "nick", ActionKind.Occupant), NotificationLevel.ALL))
    }

    @Test fun confirm_leave_dark() = shot(true, "confirm_leave_dark") {
        Box(Modifier.fillMaxWidth().padding(24.dp)) {
            ConfirmCard(
                "Leave chord-smoke?", "You will not get its messages any more. You can join again later.",
                "Leave room", "Cancel", {}, {},
            )
        }
    }
    @Test fun confirm_remove_light() = shot(false, "confirm_remove_light") {
        Box(Modifier.fillMaxWidth().padding(24.dp)) {
            ConfirmCard(
                "Remove Alice Martin?", "This person leaves your contacts. You will not see when they are online.",
                "Remove contact", "Cancel", {}, {},
            )
        }
    }

    // ---- The drawer header ----
    @Composable private fun Drawer(inbox: Int) = ChannelDrawerContent(
        spaces = DrawerFixtures.spaces, scope = ChannelScope.Home, onScope = {},
        channels = DrawerFixtures.homeChannels, loaded = true, selectedJid = "bob@chord.localhost",
        onSelect = {}, account = DrawerFixtures.account, onSignOut = {},
        spaceUnread = DrawerFixtures.spaceUnread, homeUnread = 13, inboxCount = inbox,
    )

    @Test fun drawer_inbox_badge_dark() = shot(true, "drawer_inbox_badge_dark") { Drawer(3) }
    @Test fun drawer_inbox_badge_light() = shot(false, "drawer_inbox_badge_light") { Drawer(3) }
}
