package space.foid.chord.ui.spaces

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import space.foid.chord.R
import space.foid.chord.ui.join.JoinButton
import space.foid.chord.ui.join.JoinError
import space.foid.chord.ui.join.JoinField
import space.foid.chord.ui.join.LevelRow
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import uniffi.chord_ffi.NotificationLevel

/** The top of a space sheet: the tile, the name and a line under it. */
@Composable
fun SpaceSheetHeader(target: SpaceTarget, subtitle: String, image: ImageBitmap? = null) {
    val c = Chord.colors
    Row(
        Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
    ) {
        SpaceTile(target.name, target.key, size = 44.dp, image = image)
        Column(Modifier.weight(1f)) {
            Text(target.name, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(subtitle, style = ChordType.caption, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
    Spacer(Modifier.height(ChordSpace.s3))
    SheetDivider()
}

/**
 * The menu of the space header (CircleHeader.svelte): invite, settings, create channel,
 * notifications, nickname, leave. [owner] is null while we do not know: the settings row shows.
 * It hides when the service told us that we do not own the space.
 */
@Composable
fun SpaceMenuContent(
    target: SpaceTarget,
    owner: Boolean?,
    onInvite: () -> Unit,
    onSettings: () -> Unit,
    onCreateChannel: () -> Unit,
    onNotifications: () -> Unit,
    onNickname: () -> Unit,
    onLeave: () -> Unit,
    modifier: Modifier = Modifier,
    image: ImageBitmap? = null,
) {
    Column(modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = ChordSpace.s2).testTag("space_menu")) {
        SpaceSheetHeader(target, stringResource(R.string.spaces_space_label), image)
        SpaceSheetRow(SpaceIcon.Invite, stringResource(R.string.spaces_invite), onInvite, Modifier.testTag("space_invite"))
        if (owner != false) {
            SpaceSheetRow(SpaceIcon.Settings, stringResource(R.string.spaces_settings), onSettings, Modifier.testTag("space_settings"))
        }
        SpaceSheetRow(SpaceIcon.Hash, stringResource(R.string.spaces_create_channel), onCreateChannel, Modifier.testTag("space_create_channel"))
        SheetDivider()
        SpaceSheetRow(SpaceIcon.Bell, stringResource(R.string.spaces_notifications), onNotifications, Modifier.testTag("space_notifications"))
        SpaceSheetRow(SpaceIcon.Pencil, stringResource(R.string.spaces_nickname), onNickname, Modifier.testTag("space_nickname"))
        SheetDivider()
        SpaceSheetRow(SpaceIcon.Exit, stringResource(R.string.spaces_leave), onLeave, Modifier.testTag("space_leave"), danger = true)
    }
}

/** The label of a notification level. */
@Composable
fun levelLabel(level: NotificationLevel?): String = stringResource(
    when (level) {
        NotificationLevel.MENTIONS -> R.string.actions_level_mentions
        NotificationLevel.NONE -> R.string.actions_level_none
        else -> R.string.actions_level_all
    },
)

/**
 * The menu of a long press on a space of the rail (menus.ts, circleMenu): mark as read, the
 * notification level, invite, settings, leave. [quiet] is true when nothing is unread.
 */
@Composable
fun RailMenuContent(
    target: SpaceTarget,
    owner: Boolean?,
    quiet: Boolean,
    level: NotificationLevel?,
    onMarkRead: () -> Unit,
    onLevel: (NotificationLevel) -> Unit,
    onInvite: () -> Unit,
    onSettings: () -> Unit,
    onLeave: () -> Unit,
    modifier: Modifier = Modifier,
    image: ImageBitmap? = null,
    error: String? = null,
) {
    Column(modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = ChordSpace.s2).testTag("rail_menu")) {
        SpaceSheetHeader(target, stringResource(R.string.spaces_space_label), image)
        SpaceSheetRow(
            SpaceIcon.CheckAll, stringResource(R.string.spaces_mark_read), onMarkRead, Modifier.testTag("rail_mark_read"),
            subtitle = if (quiet) stringResource(R.string.spaces_up_to_date) else null, enabled = !quiet,
        )
        SheetSectionLabel(stringResource(R.string.actions_notifications))
        LevelRow(NotificationLevel.ALL, R.string.actions_level_all, level, onLevel, "rail_level_all")
        LevelRow(NotificationLevel.MENTIONS, R.string.actions_level_mentions, level, onLevel, "rail_level_mentions")
        LevelRow(NotificationLevel.NONE, R.string.actions_level_none, level, onLevel, "rail_level_none")
        SheetDivider()
        SpaceSheetRow(SpaceIcon.Invite, stringResource(R.string.spaces_invite), onInvite, Modifier.testTag("rail_invite"))
        if (owner != false) {
            SpaceSheetRow(SpaceIcon.Settings, stringResource(R.string.spaces_settings), onSettings, Modifier.testTag("rail_settings"))
        }
        SpaceSheetRow(SpaceIcon.Exit, stringResource(R.string.spaces_leave), onLeave, Modifier.testTag("rail_leave"), danger = true)
        if (error != null) JoinError(error, "rail_error", Modifier.padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2))
    }
}

/** The menu of a long press on Home (menus.ts): mark Home as read, mark every space as read. */
@Composable
fun HomeMenuContent(
    quietHome: Boolean,
    quietAll: Boolean,
    onMarkHome: () -> Unit,
    onMarkAll: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = ChordSpace.s2).testTag("home_menu")) {
        Text(
            stringResource(R.string.spaces_home_title), style = ChordType.name, color = Chord.colors.ink,
            modifier = Modifier.padding(horizontal = ChordSpace.s4),
        )
        Spacer(Modifier.height(ChordSpace.s3))
        SheetDivider()
        SpaceSheetRow(
            SpaceIcon.CheckAll, stringResource(R.string.spaces_mark_all_read), onMarkHome, Modifier.testTag("home_mark_read"),
            subtitle = if (quietHome) stringResource(R.string.spaces_up_to_date) else null, enabled = !quietHome,
        )
        SpaceSheetRow(
            SpaceIcon.CheckAll, stringResource(R.string.spaces_mark_every_read), onMarkAll, Modifier.testTag("home_mark_every"),
            subtitle = if (quietAll) stringResource(R.string.spaces_up_to_date) else null, enabled = !quietAll,
        )
    }
}

/** A switch row, for "Mute this space". */
@Composable
internal fun SwitchRow(label: String, on: Boolean, onChange: (Boolean) -> Unit, tag: String) {
    val c = Chord.colors
    Row(
        Modifier.fillMaxWidth().height(52.dp)
            .clickable(role = Role.Switch) { onChange(!on) }
            .semantics { selected = on }
            .padding(horizontal = ChordSpace.s4)
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = ChordType.body, color = c.ink, modifier = Modifier.weight(1f))
        androidx.compose.material3.Switch(
            checked = on, onCheckedChange = null,
            colors = androidx.compose.material3.SwitchDefaults.colors(
                checkedThumbColor = c.onBrand, checkedTrackColor = c.brand,
                uncheckedThumbColor = c.inkMuted, uncheckedTrackColor = c.surface300, uncheckedBorderColor = c.lineStrong,
            ),
        )
    }
}

/**
 * "Notification settings" of a space: tell me about all, mentions or nothing, and "Mute this
 * space" as on the desktop (CircleDialog.svelte). Saving sets the level on every room.
 */
@Composable
fun SpaceNotificationsContent(
    target: SpaceTarget,
    level: NotificationLevel?,
    busy: Boolean,
    error: String?,
    onSave: (NotificationLevel) -> Unit,
    modifier: Modifier = Modifier,
) {
    var picked by remember(level) { mutableStateOf(level?.takeIf { it != NotificationLevel.NONE } ?: NotificationLevel.ALL) }
    var mute by remember(level) { mutableStateOf(level == NotificationLevel.NONE) }
    Column(modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = ChordSpace.s4).testTag("space_notifications_sheet")) {
        Text(
            stringResource(R.string.spaces_notifications), style = ChordType.title, color = Chord.colors.ink,
            modifier = Modifier.padding(horizontal = ChordSpace.s4),
        )
        Text(
            target.name, style = ChordType.bodySmall, color = Chord.colors.inkMuted,
            modifier = Modifier.padding(horizontal = ChordSpace.s4),
        )
        SheetSectionLabel(stringResource(R.string.spaces_tell_me))
        LevelRow(NotificationLevel.ALL, R.string.actions_level_all, picked, { picked = it }, "space_level_all")
        LevelRow(NotificationLevel.MENTIONS, R.string.actions_level_mentions, picked, { picked = it }, "space_level_mentions")
        LevelRow(NotificationLevel.NONE, R.string.actions_level_none, picked, { picked = it }, "space_level_none")
        SheetDivider()
        SwitchRow(stringResource(R.string.spaces_mute_space), mute, { mute = it }, "space_mute")
        if (error != null) JoinError(error, "space_error", Modifier.padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2))
        JoinButton(
            stringResource(R.string.spaces_save), { onSave(if (mute) NotificationLevel.NONE else picked) }, "space_save",
            Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2), busy = busy,
        )
    }
}

/**
 * A sheet with one text field and one button: create a channel, change the nickname, set a topic.
 * [onConfirm] gets the text. The button stays off while the text is blank and [allowBlank] is false.
 */
@Composable
fun TextPromptContent(
    title: String,
    label: String,
    initial: String,
    confirmLabel: String,
    onConfirm: (String) -> Unit,
    modifier: Modifier = Modifier,
    hint: String? = null,
    placeholder: String? = null,
    busy: Boolean = false,
    error: String? = null,
    allowBlank: Boolean = false,
    tag: String = "text_prompt",
) {
    var text by rememberSaveable { mutableStateOf(initial) }
    val focus = remember { androidx.compose.ui.focus.FocusRequester() }
    androidx.compose.runtime.LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    val go = { if ((allowBlank || text.isNotBlank()) && !busy) onConfirm(text) }
    Column(
        modifier.fillMaxWidth().navigationBarsPadding().imePadding().verticalScroll(rememberScrollState())
            .padding(horizontal = ChordSpace.s4).padding(bottom = ChordSpace.s4).testTag(tag),
    ) {
        Text(title, style = ChordType.title, color = Chord.colors.ink)
        if (hint != null) {
            Spacer(Modifier.height(ChordSpace.s1))
            Text(hint, style = ChordType.bodySmall, color = Chord.colors.inkMuted)
        }
        Spacer(Modifier.height(ChordSpace.s4))
        JoinField(
            value = text, onChange = { text = it }, label = label, tag = "${tag}_field",
            modifier = Modifier.focusRequester(focus),
            placeholder = placeholder, enabled = !busy, isError = error != null,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { go() }),
        )
        if (error != null) {
            Spacer(Modifier.height(ChordSpace.s2))
            JoinError(error, "${tag}_error")
        }
        Spacer(Modifier.height(ChordSpace.s4))
        JoinButton(
            confirmLabel, { go() }, "${tag}_confirm", Modifier.fillMaxWidth(),
            enabled = allowBlank || text.isNotBlank(), busy = busy,
        )
    }
}
