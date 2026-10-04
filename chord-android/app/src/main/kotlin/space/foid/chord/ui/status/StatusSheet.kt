package space.foid.chord.ui.status

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import space.foid.chord.R
import space.foid.chord.ui.components.PresenceBadge
import space.foid.chord.ui.components.availabilityLabel
import space.foid.chord.ui.components.toPresence
import space.foid.chord.ui.sheets.ChordModalSheet
import space.foid.chord.ui.sheets.LineIcon
import space.foid.chord.ui.sheets.ReactionPickerSheet
import space.foid.chord.ui.sheets.SheetIcon
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import uniffi.chord_ffi.Availability

/**
 * The status sheet, as the desktop status menu: availability rows, then the custom status.
 * A tap on an availability applies it and closes the sheet. Enter or Done saves the custom
 * status and closes it. A swipe down drops the edits.
 */
@Composable
fun StatusSheet(
    state: StatusState,
    onAvailability: (Availability) -> Unit,
    onStatus: (String?) -> Unit,
    onDismiss: () -> Unit,
) {
    ChordModalSheet(onDismiss) { dismissThen ->
        StatusSheetContent(
            state = state,
            onAvailability = { a -> onAvailability(a); dismissThen {} },
            onSave = { s -> onStatus(s); dismissThen {} },
            onClear = { onStatus(null) },
            modifier = Modifier.navigationBarsPadding().imePadding(),
        )
    }
}

/** The inside of [StatusSheet], with no sheet window. */
@Composable
fun StatusSheetContent(
    state: StatusState,
    onAvailability: (Availability) -> Unit,
    onSave: (String?) -> Unit,
    onClear: () -> Unit,
    modifier: Modifier = Modifier,
    initialEmojiPicker: Boolean = false,
) {
    val c = Chord.colors
    val parts = remember { splitStatus(state.status) }
    var emoji by remember { mutableStateOf(parts.emoji) }
    var text by remember { mutableStateOf(parts.text) }
    var picker by remember { mutableStateOf(initialEmojiPicker) }
    val save = { onSave(statusToSave(emoji, text)) }
    val emojiCd = if (emoji.isEmpty()) stringResource(R.string.status_add_emoji) else stringResource(R.string.status_change_emoji, emoji)
    val clearCd = stringResource(R.string.status_clear)

    Column(modifier.fillMaxWidth().padding(bottom = ChordSpace.s4).testTag("status_sheet")) {
        Text(
            stringResource(R.string.status_title),
            style = ChordType.title, color = c.ink,
            modifier = Modifier.padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2),
        )
        for (a in availabilityRows(state.canHide, state.availability)) {
            AvailabilityRow(a, selected = a == normalized(state.availability)) { onAvailability(a) }
        }
        Box(Modifier.padding(vertical = ChordSpace.s2).fillMaxWidth().height(1.dp).background(c.line))
        Row(
            Modifier
                .padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2)
                .fillMaxWidth()
                .clip(RoundedCornerShape(ChordRadius.md))
                .background(c.surface300)
                .heightIn(min = 48.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                Modifier
                    .size(48.dp)
                    .clickable(role = Role.Button) { picker = true }
                    .semantics { contentDescription = emojiCd }
                    .testTag("status_emoji"),
                contentAlignment = Alignment.Center,
            ) {
                if (emoji.isEmpty()) SmilePlus(c.inkMuted) else Text(emoji, fontSize = 22.sp)
            }
            Box(Modifier.weight(1f).padding(vertical = ChordSpace.s3), contentAlignment = Alignment.CenterStart) {
                if (text.isEmpty()) {
                    Text(stringResource(R.string.status_hint), style = ChordType.body, color = c.inkMuted)
                }
                BasicTextField(
                    value = text,
                    onValueChange = { new ->
                        // A hardware Enter saves. A paste with line breaks makes spaces.
                        if ('\n' in new && new.replace("\n", "") == text) save() else text = cleanStatusText(new)
                    },
                    textStyle = ChordType.body.copy(color = c.ink),
                    cursorBrush = SolidColor(c.brandInk),
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { save() }),
                    modifier = Modifier.fillMaxWidth().testTag("status_field"),
                )
            }
            if (emoji.isNotEmpty() || text.isNotEmpty()) {
                Box(
                    Modifier
                        .size(48.dp)
                        .clickable(role = Role.Button) { emoji = ""; text = ""; onClear() }
                        .semantics { contentDescription = clearCd }
                        .testTag("status_clear"),
                    contentAlignment = Alignment.Center,
                ) { CrossGlyph(c.inkMuted) }
            }
        }
        Text(
            stringResource(R.string.status_save_hint),
            style = ChordType.caption, color = c.inkMuted,
            modifier = Modifier.padding(horizontal = ChordSpace.s4),
        )
    }
    if (picker) ReactionPickerSheet(onDismiss = { picker = false }, onPick = { emoji = it })
}

private fun normalized(a: Availability) = if (a == Availability.EXTENDED_AWAY) Availability.AWAY else a

@Composable
private fun AvailabilityRow(a: Availability, selected: Boolean, onClick: () -> Unit) {
    val c = Chord.colors
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .clickable(role = Role.RadioButton, onClick = onClick)
            .semantics { this.selected = selected }
            .padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2)
            .testTag("status_${a.name.lowercase()}"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(24.dp), contentAlignment = Alignment.CenterStart) {
            PresenceBadge(a.toPresence(), size = 12.dp)
        }
        Spacer(Modifier.width(ChordSpace.s3))
        Column(Modifier.weight(1f)) {
            Text(availabilityLabel(a), style = ChordType.body, color = c.ink)
            if (a == Availability.INVISIBLE) {
                Text(stringResource(R.string.status_invisible_hint), style = ChordType.bodySmall, color = c.inkMuted)
            }
        }
        if (selected) LineIcon(SheetIcon.Check, c.brandInk, 22.dp)
    }
}

@Composable
private fun CrossGlyph(color: Color) {
    Canvas(Modifier.size(16.dp)) {
        val w = 1.8.dp.toPx()
        drawLine(color, Offset(2.dp.toPx(), 2.dp.toPx()), Offset(size.width - 2.dp.toPx(), size.height - 2.dp.toPx()), w, StrokeCap.Round)
        drawLine(color, Offset(size.width - 2.dp.toPx(), 2.dp.toPx()), Offset(2.dp.toPx(), size.height - 2.dp.toPx()), w, StrokeCap.Round)
    }
}

/** A smile with a plus: "add a status emoji". */
@Composable
private fun SmilePlus(color: Color) {
    Canvas(Modifier.size(22.dp)) {
        val k = size.width / 24f
        val st = Stroke(1.8f * k, cap = StrokeCap.Round)
        drawCircle(color, 9 * k, Offset(11 * k, 13 * k), style = st)
        drawArc(color, 20f, 140f, false, Offset(7 * k, 9.5f * k), androidx.compose.ui.geometry.Size(8 * k, 8 * k), style = st)
        drawCircle(color, 0.9f * k, Offset(8.5f * k, 11 * k))
        drawCircle(color, 0.9f * k, Offset(13.5f * k, 11 * k))
        drawLine(color, Offset(19 * k, 2 * k), Offset(19 * k, 8 * k), st.width, StrokeCap.Round)
        drawLine(color, Offset(16 * k, 5 * k), Offset(22 * k, 5 * k), st.width, StrokeCap.Round)
    }
}
