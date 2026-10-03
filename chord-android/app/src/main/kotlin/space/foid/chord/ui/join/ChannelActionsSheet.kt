package space.foid.chord.ui.join

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import space.foid.chord.R
import space.foid.chord.ui.components.Avatar
import space.foid.chord.ui.sheets.ChordModalSheet
import space.foid.chord.ui.spaces.MuteDuration
import space.foid.chord.ui.spaces.MuteStatus
import space.foid.chord.ui.spaces.SheetDivider
import space.foid.chord.ui.spaces.SheetSectionLabel
import space.foid.chord.ui.spaces.SpaceGlyph
import space.foid.chord.ui.spaces.SpaceIcon
import space.foid.chord.ui.spaces.SpaceSheetRow
import space.foid.chord.ui.spaces.TextPromptContent
import space.foid.chord.ui.spaces.levelLabel
import space.foid.chord.ui.spaces.muteStatus
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.ActionKind
import space.foid.chord.viewmodel.ActionsState
import space.foid.chord.viewmodel.ChannelActionTarget
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.NotificationSetting

/** The pages of the channel sheet. The sub pages are the submenus of the desktop. */
enum class ChannelView { Main, Level, Mute, Topic, Settings }

/**
 * The sheet of a channel row (long press), as the desktop menu: mark as read, the notification
 * level, mute with a duration, set the topic, channel settings, copy the address and link, and
 * leave the room or remove the contact (after a question).
 * Each action that ends the sheet hides it first, then runs and calls [onDismiss].
 */
@Composable
fun ChannelActionsSheet(
    state: ActionsState,
    onDismiss: () -> Unit,
    onMarkRead: () -> Unit,
    onLevel: (NotificationLevel) -> Unit,
    onCopy: () -> Unit,
    onLeave: () -> Unit,
    onMute: (MuteDuration) -> Unit = {},
    onUnmute: () -> Unit = {},
    onCopyLink: () -> Unit = {},
    onSaveChannel: (topic: String?, name: String?, onDone: () -> Unit) -> Unit = { _, _, done -> done() },
) {
    var confirming by remember { mutableStateOf(false) }
    ChordModalSheet(onDismiss) { dismissThen ->
        ChannelActionsContent(
            state = state,
            onMarkRead = { dismissThen(onMarkRead) },
            onLevel = onLevel,
            onCopy = { dismissThen(onCopy) },
            onLeave = { confirming = true },
            onMute = { d -> dismissThen { onMute(d) } },
            onUnmute = { dismissThen(onUnmute) },
            onCopyLink = { dismissThen(onCopyLink) },
            onSaveTopic = { t -> onSaveChannel(t, null) { dismissThen {} } },
            onSaveSettings = { name, topic -> onSaveChannel(topic, name) { dismissThen {} } },
        )
        if (confirming) {
            ConfirmDialog(
                title = leaveTitle(state.target.kind, state.target.name),
                text = leaveText(state.target.kind),
                confirmLabel = leaveLabel(state.target.kind),
                cancelLabel = stringResource(R.string.actions_cancel),
                onConfirm = { confirming = false; onLeave() },
                onCancel = { confirming = false },
            )
        }
    }
}

@Composable
private fun leaveTitle(kind: ActionKind, name: String) =
    stringResource(if (kind == ActionKind.Room) R.string.actions_leave_title else R.string.actions_remove_title, name)

@Composable
private fun leaveText(kind: ActionKind) =
    stringResource(if (kind == ActionKind.Room) R.string.actions_leave_text else R.string.actions_remove_text)

@Composable
private fun leaveLabel(kind: ActionKind) =
    stringResource(if (kind == ActionKind.Room) R.string.actions_leave else R.string.actions_remove)

/** The inside of [ChannelActionsSheet], with no sheet window. [now] is the Unix time in ms, for a timed mute. */
@Composable
fun ChannelActionsContent(
    state: ActionsState,
    onMarkRead: () -> Unit,
    onLevel: (NotificationLevel) -> Unit,
    onCopy: () -> Unit,
    onLeave: () -> Unit,
    modifier: Modifier = Modifier,
    onMute: (MuteDuration) -> Unit = {},
    onUnmute: () -> Unit = {},
    onCopyLink: () -> Unit = {},
    onSaveTopic: (String) -> Unit = {},
    onSaveSettings: (name: String?, topic: String?) -> Unit = { _, _ -> },
    initialView: ChannelView = ChannelView.Main,
    now: Long = System.currentTimeMillis(),
) {
    var view by remember { mutableStateOf(initialView) }
    val target = state.target
    val box = modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = ChordSpace.s2)
    when (view) {
        ChannelView.Main -> MainPage(state, now, box, onMarkRead, onCopy, onCopyLink, onUnmute, onLeave) { view = it }
        ChannelView.Level -> Column(box.testTag("channel_level_page")) {
            BackRow(stringResource(R.string.actions_notifications)) { view = ChannelView.Main }
            LevelRow(NotificationLevel.ALL, R.string.actions_level_all, state.level, onLevel, "level_all")
            LevelRow(NotificationLevel.MENTIONS, R.string.actions_level_mentions, state.level, onLevel, "level_mentions")
            LevelRow(NotificationLevel.NONE, R.string.actions_level_none, state.level, onLevel, "level_none")
            ErrorLine(state)
        }
        ChannelView.Mute -> Column(box.testTag("channel_mute_page")) {
            BackRow(stringResource(if (target.kind == ActionKind.Room) R.string.actions_mute_channel else R.string.actions_mute_chat)) {
                view = ChannelView.Main
            }
            MuteDuration.entries.forEach { d ->
                SpaceSheetRow(
                    SpaceIcon.Clock, stringResource(muteLabel(d)), { onMute(d) },
                    Modifier.testTag("mute_${d.name}"),
                )
            }
            ErrorLine(state)
        }
        ChannelView.Topic -> Column(box) {
            TextPromptContent(
                title = stringResource(R.string.actions_set_topic),
                label = stringResource(R.string.actions_topic_label),
                initial = "",
                confirmLabel = stringResource(R.string.spaces_save),
                onConfirm = onSaveTopic,
                hint = stringResource(R.string.actions_topic_hint),
                busy = state.busy, error = state.error, allowBlank = true, tag = "topic_prompt",
            )
        }
        ChannelView.Settings -> ChannelSettingsPage(state, box, onSaveSettings)
    }
}

@Composable
private fun MainPage(
    state: ActionsState,
    now: Long,
    modifier: Modifier,
    onMarkRead: () -> Unit,
    onCopy: () -> Unit,
    onCopyLink: () -> Unit,
    onUnmute: () -> Unit,
    onLeave: () -> Unit,
    go: (ChannelView) -> Unit,
) {
    val c = Chord.colors
    val target = state.target
    val room = target.kind == ActionKind.Room
    val mute = muteStatus(state.level?.let { NotificationSetting(it, state.muteUntil) }, now)
    Column(modifier.verticalScroll(rememberScrollState()).testTag("channel_actions_sheet")) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
        ) {
            Avatar(target.address, name = target.name, size = 40.dp, cut = c.surface200)
            Column(Modifier.weight(1f)) {
                Text(target.name, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(target.address, style = ChordType.caption, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Spacer(Modifier.height(ChordSpace.s3))
        SheetDivider()
        SpaceSheetRow(SpaceIcon.CheckAll, stringResource(R.string.actions_mark_read), onMarkRead, Modifier.testTag("action_mark_read"))
        SpaceSheetRow(
            SpaceIcon.Bell, stringResource(R.string.actions_notification_level), { go(ChannelView.Level) },
            Modifier.testTag("action_level"), subtitle = state.level?.let { levelLabel(it) }, chevron = true,
        )
        if (mute == MuteStatus.Off) {
            SpaceSheetRow(
                SpaceIcon.BellOff,
                stringResource(if (room) R.string.actions_mute_channel else R.string.actions_mute_chat),
                { go(ChannelView.Mute) }, Modifier.testTag("action_mute"), chevron = true,
            )
        } else {
            SpaceSheetRow(
                SpaceIcon.Bell,
                stringResource(if (room) R.string.actions_unmute_channel else R.string.actions_unmute_chat),
                onUnmute, Modifier.testTag("action_unmute"), subtitle = muteText(mute),
            )
        }
        if (room) {
            SpaceSheetRow(SpaceIcon.Topic, stringResource(R.string.actions_set_topic), { go(ChannelView.Topic) }, Modifier.testTag("action_topic"))
            SpaceSheetRow(SpaceIcon.Settings, stringResource(R.string.actions_channel_settings), { go(ChannelView.Settings) }, Modifier.testTag("action_settings"))
        }
        SheetDivider()
        SpaceSheetRow(SpaceIcon.Copy, stringResource(R.string.actions_copy), onCopy, Modifier.testTag("action_copy"))
        if (room) SpaceSheetRow(SpaceIcon.Link, stringResource(R.string.actions_copy_link), onCopyLink, Modifier.testTag("action_copy_link"))
        if (target.kind != ActionKind.Occupant) {
            SpaceSheetRow(
                SpaceIcon.Exit,
                stringResource(if (target.kind == ActionKind.Room) R.string.actions_leave else R.string.actions_remove),
                onLeave, Modifier.testTag("action_leave"), danger = true,
            )
        }
        ErrorLine(state)
    }
}

@Composable
private fun muteText(mute: MuteStatus): String? = when (mute) {
    MuteStatus.Off -> null
    MuteStatus.Forever -> stringResource(R.string.actions_muted_forever)
    is MuteStatus.Until -> {
        val time = android.text.format.DateFormat.getTimeFormat(LocalContext.current).format(java.util.Date(mute.at))
        stringResource(R.string.actions_muted_until, time)
    }
}

private fun muteLabel(d: MuteDuration): Int = when (d) {
    MuteDuration.Minutes15 -> R.string.actions_mute_15m
    MuteDuration.Hour1 -> R.string.actions_mute_1h
    MuteDuration.Hours8 -> R.string.actions_mute_8h
    MuteDuration.Hours24 -> R.string.actions_mute_24h
    MuteDuration.Forever -> R.string.actions_mute_forever
}

@Composable
private fun ErrorLine(state: ActionsState) {
    if (state.error != null) {
        JoinError(state.error, "actions_error", Modifier.padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2))
    }
}

/** A row with an arrow, to go back to the main page of a sheet. */
@Composable
internal fun BackRow(title: String, onBack: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(52.dp)
            .clickable(role = Role.Button, onClick = onBack)
            .padding(horizontal = ChordSpace.s4)
            .testTag("sheet_back"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SpaceGlyph(SpaceIcon.Back, Chord.colors.ink)
        Spacer(Modifier.width(ChordSpace.s4))
        Text(title, style = ChordType.name, color = Chord.colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
    SheetDivider()
}

/** The settings page of a channel: its name and a topic. Only an owner can change the name. */
@Composable
private fun ChannelSettingsPage(state: ActionsState, modifier: Modifier, onSave: (name: String?, topic: String?) -> Unit) {
    var name by remember { mutableStateOf(state.target.name) }
    var topic by remember { mutableStateOf("") }
    Column(
        modifier.imePadding().verticalScroll(rememberScrollState()).padding(horizontal = ChordSpace.s4).testTag("channel_settings_page"),
    ) {
        Text(stringResource(R.string.actions_channel_settings), style = ChordType.title, color = Chord.colors.ink)
        Spacer(Modifier.height(ChordSpace.s4))
        JoinField(name, { name = it }, stringResource(R.string.actions_channel_name), "settings_name", enabled = !state.busy)
        Spacer(Modifier.height(ChordSpace.s3))
        JoinField(
            topic, { topic = it }, stringResource(R.string.actions_topic_label), "settings_topic",
            enabled = !state.busy, placeholder = stringResource(R.string.actions_topic_keep),
        )
        Text(
            stringResource(R.string.actions_settings_hint), style = ChordType.bodySmall, color = Chord.colors.inkMuted,
            modifier = Modifier.padding(top = ChordSpace.s2),
        )
        if (state.error != null) {
            Spacer(Modifier.height(ChordSpace.s2))
            JoinError(state.error, "settings_error")
        }
        Spacer(Modifier.height(ChordSpace.s4))
        val newName = name.trim().takeIf { it.isNotEmpty() && it != state.target.name }
        val newTopic = topic.takeIf { it.isNotBlank() }
        JoinButton(
            stringResource(R.string.spaces_save), { onSave(newName, newTopic) }, "settings_save", Modifier.fillMaxWidth(),
            enabled = (newName != null || newTopic != null) && !state.busy, busy = state.busy,
        )
    }
}

@Composable
internal fun LevelRow(
    level: NotificationLevel,
    label: Int,
    current: NotificationLevel?,
    onLevel: (NotificationLevel) -> Unit,
    tag: String,
) = RadioRow(stringResource(label), current == level, { onLevel(level) }, tag)

/** A row with a round mark that is on or off. */
@Composable
internal fun RadioRow(label: String, on: Boolean, onClick: () -> Unit, tag: String) {
    val c = Chord.colors
    Row(
        Modifier.fillMaxWidth().height(48.dp)
            .clickable(role = Role.RadioButton, onClick = onClick)
            .semantics { selected = on }
            .padding(horizontal = ChordSpace.s4)
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier.size(22.dp).clip(CircleShape).drawBehind {
                drawCircle(if (on) c.brand else c.lineStrong, radius = size.minDimension / 2 - 1.dp.toPx(), style = Stroke(2.dp.toPx()))
                if (on) drawCircle(c.brand, radius = 5.dp.toPx(), center = Offset(size.width / 2, size.height / 2))
            },
        )
        Spacer(Modifier.width(ChordSpace.s4))
        Text(label, style = ChordType.body, color = if (on) c.ink else c.inkMuted)
    }
}

/** What the actions sheet needs to know about a row of the channel list. */
fun ChannelItem.actionTarget(): ChannelActionTarget = ChannelActionTarget(
    jid = jid,
    name = name.ifBlank { jid.substringBefore('/') },
    kind = when (kind) {
        is ChannelKind.Direct -> ActionKind.Contact
        is ChannelKind.PrivateMessage -> ActionKind.Occupant
        else -> ActionKind.Room
    },
)
