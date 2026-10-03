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
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
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
import space.foid.chord.ui.sheets.SheetIcon
import space.foid.chord.ui.sheets.SheetRow
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.ActionKind
import space.foid.chord.viewmodel.ChannelActionTarget
import space.foid.chord.viewmodel.ActionsState
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.NotificationLevel

/**
 * The sheet of a channel row (long press): mark as read, the notification level, copy the
 * address, and leave the room or remove the contact (after a question).
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
) {
    var confirming by remember { mutableStateOf(false) }
    ChordModalSheet(onDismiss) { dismissThen ->
        ChannelActionsContent(
            state = state,
            onMarkRead = { dismissThen(onMarkRead) },
            onLevel = onLevel,
            onCopy = { dismissThen(onCopy) },
            onLeave = { confirming = true },
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

/** The inside of [ChannelActionsSheet], with no sheet window. */
@Composable
fun ChannelActionsContent(
    state: ActionsState,
    onMarkRead: () -> Unit,
    onLevel: (NotificationLevel) -> Unit,
    onCopy: () -> Unit,
    onLeave: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    val target = state.target
    Column(modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = ChordSpace.s2).testTag("channel_actions_sheet")) {
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
        Divider()
        SheetRow(SheetIcon.Search, stringResource(R.string.actions_mark_read), onMarkRead, Modifier.testTag("action_mark_read"))
        Text(
            stringResource(R.string.actions_notifications).uppercase(),
            style = ChordType.caption, color = c.inkMuted,
            modifier = Modifier.padding(start = ChordSpace.s4, top = ChordSpace.s3, bottom = ChordSpace.s1),
        )
        LevelRow(NotificationLevel.ALL, R.string.actions_level_all, state.level, onLevel, "level_all")
        LevelRow(NotificationLevel.MENTIONS, R.string.actions_level_mentions, state.level, onLevel, "level_mentions")
        LevelRow(NotificationLevel.NONE, R.string.actions_level_none, state.level, onLevel, "level_none")
        Divider()
        SheetRow(SheetIcon.Copy, stringResource(R.string.actions_copy), onCopy, Modifier.testTag("action_copy"))
        if (target.kind != ActionKind.Occupant) {
            SheetRow(
                SheetIcon.Delete,
                stringResource(if (target.kind == ActionKind.Room) R.string.actions_leave else R.string.actions_remove),
                onLeave, Modifier.testTag("action_leave"), danger = true,
            )
        }
        if (state.error != null) {
            JoinError(state.error, "actions_error", Modifier.padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2))
        }
    }
}

@Composable
private fun Divider() {
    Box(Modifier.fillMaxWidth().height(1.dp).background(Chord.colors.line))
}

@Composable
private fun LevelRow(
    level: NotificationLevel,
    label: Int,
    current: NotificationLevel?,
    onLevel: (NotificationLevel) -> Unit,
    tag: String,
) {
    val c = Chord.colors
    val on = current == level
    Row(
        Modifier.fillMaxWidth().height(48.dp)
            .clickable(role = Role.RadioButton) { onLevel(level) }
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
        Text(stringResource(label), style = ChordType.body, color = if (on) c.ink else c.inkMuted)
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
