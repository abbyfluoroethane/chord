package space.foid.chord.ui.spaces

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
import space.foid.chord.ui.join.ChannelActionsContent
import space.foid.chord.ui.join.ChannelView
import space.foid.chord.ui.screens.ChannelDrawerContent
import space.foid.chord.ui.screens.DrawerFixtures
import space.foid.chord.ui.sheets.SheetFrame
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.viewmodel.ActionKind
import space.foid.chord.viewmodel.ActionsState
import space.foid.chord.viewmodel.ChannelActionTarget
import space.foid.chord.viewmodel.SpaceRow
import space.foid.chord.viewmodel.SpaceState
import space.foid.chord.viewmodel.SpaceStatus
import space.foid.chord.viewmodel.SpacesState
import space.foid.chord.viewmodel.contact
import space.foid.chord.viewmodel.room
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.JoinRequest
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.SpaceAccess
import uniffi.chord_ffi.SpaceInfo
import uniffi.chord_ffi.SpaceMember

// The sheet window is not drawn here: the tests draw the content in a frame styled like the sheet.
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h700dp-xxhdpi")
class SpacesScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val target = SpaceTarget("pubsub.chat.foid.space", "launch-ops", "Launch Ops")
    private val general = "launch-ops-general@conference.chat.foid.space"
    private val now = 1_800_000_000_000L

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent {
            ChordTheme(dark = dark) {
                Box(Modifier.fillMaxSize().background(Chord.colors.surface100)) {
                    Box(Modifier.fillMaxSize().background(Chord.colors.scrim))
                    Box(Modifier.align(Alignment.BottomCenter)) { content() }
                }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/spaces/$name.png")
    }

    private fun page(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { content() } }
        compose.onRoot().captureRoboImage("src/test/screenshots/spaces/$name.png")
    }

    private fun both(name: String, content: @Composable () -> Unit) {
        shot(true, "${name}_dark", content)
    }

    @Composable private fun Sheet(content: @Composable () -> Unit) = SheetFrame { content() }

    // ---- The space header menu ----
    @Composable private fun Menu(owner: Boolean?) = Sheet {
        SpaceMenuContent(target, owner, {}, {}, {}, {}, {}, {})
    }
    @Test fun menu_owner_dark() = shot(true, "menu_owner_dark") { Menu(true) }
    @Test fun menu_owner_light() = shot(false, "menu_owner_light") { Menu(true) }
    @Test fun menu_member_dark() = shot(true, "menu_member_dark") { Menu(false) }
    @Test fun menu_member_light() = shot(false, "menu_member_light") { Menu(false) }

    // ---- The rail menus ----
    @Composable private fun Rail(level: NotificationLevel?, error: String? = null) = Sheet {
        RailMenuContent(target, true, quiet = false, level = level, {}, {}, {}, {}, {}, error = error)
    }
    @Test fun rail_menu_dark() = shot(true, "rail_menu_dark") { Rail(NotificationLevel.MENTIONS) }
    @Test fun rail_menu_light() = shot(false, "rail_menu_light") { Rail(NotificationLevel.ALL) }
    @Test fun rail_menu_error_dark() = shot(true, "rail_menu_error_dark") { Rail(NotificationLevel.ALL, "The server refused the request.") }
    @Composable private fun Home(quiet: Boolean) = Sheet { HomeMenuContent(quiet, quiet, {}, {}) }
    @Test fun home_menu_dark() = shot(true, "home_menu_dark") { Home(false) }
    @Test fun home_menu_light() = shot(false, "home_menu_light") { Home(false) }
    @Test fun home_menu_quiet_dark() = shot(true, "home_menu_quiet_dark") { Home(true) }

    // ---- Notification settings ----
    @Test fun notifications_dark() = shot(true, "notifications_dark") {
        Sheet { SpaceNotificationsContent(target, NotificationLevel.MENTIONS, false, null, {}) }
    }
    @Test fun notifications_light() = shot(false, "notifications_light") {
        Sheet { SpaceNotificationsContent(target, NotificationLevel.ALL, false, null, {}) }
    }
    @Test fun notifications_muted_dark() = shot(true, "notifications_muted_dark") {
        Sheet { SpaceNotificationsContent(target, NotificationLevel.NONE, false, "The server refused the request.", {}) }
    }

    // ---- Text sheets ----
    @Test fun create_channel_dark() = shot(true, "create_channel_dark") {
        Sheet {
            TextPromptContent("Create a channel", "Channel name", "", "Create channel", {}, hint = "A new channel in Launch Ops.", placeholder = "general")
        }
    }
    @Test fun create_channel_light() = shot(false, "create_channel_light") {
        Sheet {
            TextPromptContent("Create a channel", "Channel name", "Game Night", "Create channel", {}, hint = "A new channel in Launch Ops.")
        }
    }
    @Test fun create_channel_error_dark() = shot(true, "create_channel_error_dark") {
        Sheet {
            TextPromptContent(
                "Create a channel", "Channel name", "Game Night", "Create channel", {}, hint = "A new channel in Launch Ops.",
                error = "Only the owner of this space can do this.",
            )
        }
    }
    @Test fun nickname_dark() = shot(true, "nickname_dark") {
        Sheet { TextPromptContent("Change nickname", "Nickname", "Abby", "Save", {}, hint = "The name that people see in the channels of Launch Ops.") }
    }
    @Test fun nickname_light() = shot(false, "nickname_light") {
        Sheet { TextPromptContent("Change nickname", "Nickname", "Abby", "Save", {}, busy = true) }
    }

    // ---- Add a space ----
    private val listed = SpacesState.Loaded(
        listOf(
            SpaceRow(SpaceInfo("pubsub.chat.foid.space", "hikers", "Weekend Hikers", "Trails, gear and Sunday plans.", "open")),
            SpaceRow(SpaceInfo("pubsub.chat.foid.space", "synth", "Synth Corner", "Patches and modular talk.", "open"), SpaceStatus.Joining),
            SpaceRow(SpaceInfo("pubsub.chat.foid.space", "launch-ops", "Launch Ops", null, "open")),
            SpaceRow(SpaceInfo("pubsub.chat.foid.space", "club", "Closed Club", "Members only.", "whitelist"), SpaceStatus.Requested),
        ),
    )
    @Composable private fun Add(ui: AddSpaceUi, spaces: SpacesState = SpacesState.Idle) = Sheet {
        AddSpaceContent(ui, spaces, setOf(target.key), AddSpaceCallbacks())
    }
    @Test fun add_create_dark() = shot(true, "add_create_dark") { Add(AddSpaceUi(name = "Launch Ops")) }
    @Test fun add_create_light() = shot(false, "add_create_light") { Add(AddSpaceUi(access = SpaceAccess.AUTHORIZE)) }
    @Test fun add_create_error_dark() = shot(true, "add_create_error_dark") {
        Add(AddSpaceUi(name = "Launch Ops", error = "This server does not let you do this."))
    }
    @Test fun add_join_dark() = shot(true, "add_join_dark") { Add(AddSpaceUi(tab = AddSpaceTab.Join), listed) }
    @Test fun add_join_light() = shot(false, "add_join_light") { Add(AddSpaceUi(tab = AddSpaceTab.Join), listed) }
    @Test fun add_join_search_dark() = shot(true, "add_join_search_dark") { Add(AddSpaceUi(tab = AddSpaceTab.Join, query = "syn"), listed) }
    @Test fun add_join_loading_dark() = shot(true, "add_join_loading_dark") { Add(AddSpaceUi(tab = AddSpaceTab.Join), SpacesState.Loading) }

    // ---- Invite page ----
    private val contacts = listOf(
        contact("rin@chat.foid.space", "Rin"), contact("sam@chat.foid.space"), contact("jo@chat.foid.space", "Jo Lee"),
        contact("lee@chat.foid.space", "Lee"),
    )
    @Composable private fun Invite(picked: Set<String>, query: String = "", copied: Boolean = false, list: List<uniffi.chord_ffi.Contact>? = contacts) =
        SpacePageFrame("Invite people", {}, bottom = { InviteButton(picked.size, false) {} }) {
            InviteContent(target, list, picked, query, copied, null, {}, {}, {})
        }
    @Test fun invite_dark() = page(true, "invite_dark") { Invite(setOf("rin@chat.foid.space", "jo@chat.foid.space")) }
    @Test fun invite_light() = page(false, "invite_light") { Invite(emptySet()) }
    @Test fun invite_copied_dark() = page(true, "invite_copied_dark") { Invite(setOf("sam@chat.foid.space"), query = "s", copied = true) }
    @Test fun invite_no_contacts_dark() = page(true, "invite_no_contacts_dark") { Invite(emptySet(), list = emptyList()) }

    // ---- Settings page ----
    private val settingsState = SpaceState(
        target = target, owner = true, settingsLoaded = true, description = "Launch checklists and flight notes.",
        members = listOf(SpaceMember("abby@chat.foid.space", "owner"), SpaceMember("rin@chat.foid.space", "member"), SpaceMember("troll@chat.foid.space", "outcast")),
        requests = listOf(JoinRequest("pat@chat.foid.space", null)),
        channels = listOf(room(general, "general"), room("launch-ops-pad@conference.chat.foid.space", "pad")),
    )
    @Composable private fun Settings(state: SpaceState) =
        SpacePageFrame("Space settings", {}, bottom = if (state.owner == true) ({ SettingsSaveButton(true, false) {} }) else null) {
            SettingsContent(state, target.name, state.description, SettingsCallbacks())
        }
    @Test fun settings_owner_dark() = page(true, "settings_owner_dark") { Settings(settingsState) }
    @Test fun settings_owner_light() = page(false, "settings_owner_light") { Settings(settingsState) }
    @Test fun settings_member_dark() = page(true, "settings_member_dark") { Settings(SpaceState(target = target, owner = false, settingsLoaded = true)) }
    @Test fun settings_error_dark() = page(true, "settings_error_dark") { Settings(settingsState.copy(error = "Only the owner of this space can do this.")) }

    // ---- The channel menu ----
    private val roomTarget = ChannelActionTarget(general, "general", ActionKind.Room)
    private val dmTarget = ChannelActionTarget("rin@chat.foid.space", "Rin", ActionKind.Contact)
    @Composable private fun Actions(state: ActionsState, view: ChannelView = ChannelView.Main) = Sheet {
        ChannelActionsContent(state, {}, {}, {}, {}, initialView = view, now = now)
    }
    @Test fun channel_main_dark() = shot(true, "channel_main_dark") { Actions(ActionsState(roomTarget, NotificationLevel.ALL)) }
    @Test fun channel_main_light() = shot(false, "channel_main_light") { Actions(ActionsState(roomTarget, NotificationLevel.MENTIONS)) }
    @Test fun channel_muted_dark() = shot(true, "channel_muted_dark") {
        Actions(ActionsState(roomTarget, NotificationLevel.ALL, muteUntil = now + 3_600_000))
    }
    @Test fun channel_muted_forever_light() = shot(false, "channel_muted_forever_light") {
        Actions(ActionsState(roomTarget, NotificationLevel.NONE))
    }
    @Test fun channel_dm_dark() = shot(true, "channel_dm_dark") { Actions(ActionsState(dmTarget, NotificationLevel.ALL)) }
    @Test fun channel_level_dark() = shot(true, "channel_level_dark") { Actions(ActionsState(roomTarget, NotificationLevel.MENTIONS), ChannelView.Level) }
    @Test fun channel_mute_dark() = shot(true, "channel_mute_dark") { Actions(ActionsState(roomTarget, NotificationLevel.ALL), ChannelView.Mute) }
    @Test fun channel_mute_light() = shot(false, "channel_mute_light") { Actions(ActionsState(roomTarget, NotificationLevel.ALL), ChannelView.Mute) }
    @Test fun channel_topic_dark() = shot(true, "channel_topic_dark") { Actions(ActionsState(roomTarget, NotificationLevel.ALL), ChannelView.Topic) }
    @Test fun channel_settings_dark() = shot(true, "channel_settings_dark") { Actions(ActionsState(roomTarget, NotificationLevel.ALL), ChannelView.Settings) }
    @Test fun channel_settings_light() = shot(false, "channel_settings_light") {
        Actions(ActionsState(roomTarget, NotificationLevel.ALL, error = "Only the owner of this space can do this."), ChannelView.Settings)
    }

    // ---- The drawer: rail, header and the plus buttons ----
    @Composable private fun Drawer() = ChannelDrawerContent(
        spaces = DrawerFixtures.spaces, scope = ChannelScope.Space("muc.chord.localhost", "design"), onScope = {},
        channels = DrawerFixtures.spaceChannels, loaded = true, selectedJid = "general@muc.chord.localhost",
        onSelect = {}, account = DrawerFixtures.account, onSignOut = {},
        spaceUnread = DrawerFixtures.spaceUnread, onCreateChannel = {},
    )
    @Test fun drawer_space_dark() = page(true, "drawer_space_dark") { Drawer() }
    @Test fun drawer_space_light() = page(false, "drawer_space_light") { Drawer() }
}
