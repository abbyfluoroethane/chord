package space.foid.chord.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/**
 * The composer: pinned to the bottom of the chat screen, above the keyboard.
 *
 * INSETS. The bar applies the bottom system insets itself, in this order: `imePadding()`, then
 * `navigationBarsPadding()`. Compose consumes what the first one used, so the bar clears the
 * keyboard when it is open, and the navigation bar when it is closed, never both. The bar draws
 * its own background behind the inset, so the area under the navigation bar has the bar colour.
 *
 * The caller must NOT also apply any of these on the bar or on a parent of the bar:
 * `imePadding()`, `navigationBarsPadding()`, `safeDrawingPadding()`, `systemBarsPadding()`,
 * `windowInsetsPadding(WindowInsets.ime / navigationBars / safeDrawing)`, and must not use
 * `Scaffold` bottom content padding for the bar. The Activity is edge-to-edge
 * (`enableEdgeToEdge()`) and the manifest has `windowSoftInputMode="adjustResize"` or the
 * default: do not use `adjustPan`. Put the bar as the last child of a `Column` below the
 * timeline (`weight(1f)`); the timeline shrinks when the bar grows. The caller may add the
 * top and side insets (`statusBarsPadding`, `displayCutoutPadding`) on its own.
 *
 * The text is hoisted. The bar is 6 lines high at most, then the field scrolls.
 *
 * @param replyingTo the name of the sender of the quoted message: shows the "Replying to" strip.
 * @param editing true while the user edits an own message: shows the "Editing message" strip.
 *   [onCancelEdit] must clear the text too, the bar does not touch it.
 * @param onAttach opens the file picker. The bar only calls it.
 */
@Composable
fun ComposerBar(
    text: String,
    onTextChange: (String) -> Unit,
    onSend: () -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "Message",
    replyingTo: String? = null,
    onCancelReply: () -> Unit = {},
    editing: Boolean = false,
    onCancelEdit: () -> Unit = {},
    onAttach: () -> Unit = {},
) {
    val colors = Chord.colors
    val canSend = text.isNotBlank()
    Column(
        modifier
            .fillMaxWidth()
            .background(colors.surface100)
            .imePadding()
            .navigationBarsPadding(),
    ) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(colors.line))
        if (replyingTo != null) {
            Strip(onCancel = onCancelReply, cancelLabel = "Cancel reply") {
                Text("Replying to ", style = ChordType.bodySmall, color = colors.inkMuted)
                Text(
                    replyingTo,
                    style = ChordType.bodySmall.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold),
                    color = colors.ink,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
            }
        }
        if (editing) {
            Strip(onCancel = onCancelEdit, cancelLabel = "Cancel edit") {
                Text("Editing message", style = ChordType.bodySmall, color = colors.brandInk)
            }
        }
        Row(
            Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s2, vertical = ChordSpace.s2),
            verticalAlignment = Alignment.Bottom,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
        ) {
            RoundButton(label = "Attach a file", onClick = onAttach, background = colors.surface300) { PlusGlyph(colors.inkMuted) }
            BasicTextField(
                value = text,
                onValueChange = onTextChange,
                modifier = Modifier.weight(1f),
                textStyle = ChordType.body.copy(color = colors.ink),
                cursorBrush = SolidColor(colors.brand),
                maxLines = 6,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                decorationBox = { inner ->
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .heightIn(min = ChordSize.avatar)
                            .background(colors.surface300, RoundedCornerShape(20.dp))
                            .border(1.dp, colors.line, RoundedCornerShape(20.dp))
                            .padding(horizontal = ChordSpace.s4, vertical = 9.dp),
                        contentAlignment = Alignment.CenterStart,
                    ) {
                        if (text.isEmpty()) Text(placeholder, style = ChordType.body, color = colors.inkMuted)
                        inner()
                    }
                },
            )
            RoundButton(
                label = if (editing) "Save edit" else "Send",
                onClick = { if (canSend) onSend() },
                background = if (canSend) colors.brand else colors.surface300,
                enabled = canSend,
            ) { SendGlyph(if (canSend) colors.onBrand else colors.inkMuted) }
        }
    }
}

/** A thin strip above the field: a label slot and a cancel button. */
@Composable
private fun Strip(
    onCancel: () -> Unit,
    cancelLabel: String,
    content: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit,
) {
    val colors = Chord.colors
    Row(
        Modifier.fillMaxWidth().background(colors.surface200).padding(start = ChordSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) { content() }
        Box(
            Modifier
                .size(40.dp)
                .semantics { contentDescription = cancelLabel; role = Role.Button }
                .clickable(onClick = onCancel),
            contentAlignment = Alignment.Center,
        ) { CloseGlyph(colors.inkMuted) }
    }
}

@Composable
private fun RoundButton(
    label: String,
    onClick: () -> Unit,
    background: Color,
    enabled: Boolean = true,
    glyph: @Composable () -> Unit,
) {
    Box(
        Modifier
            .size(ChordSize.avatar)
            .background(background, CircleShape)
            .semantics { contentDescription = label; role = Role.Button }
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { glyph() }
}

// The glyphs are drawn, so the app needs no icon library.

@Composable
private fun PlusGlyph(color: Color) {
    Box(
        Modifier.size(18.dp).drawBehind {
            val w = 2.dp.toPx()
            drawLine(color, Offset(0f, size.height / 2), Offset(size.width, size.height / 2), w, StrokeCap.Round)
            drawLine(color, Offset(size.width / 2, 0f), Offset(size.width / 2, size.height), w, StrokeCap.Round)
        },
    )
}

@Composable
private fun CloseGlyph(color: Color) {
    Box(
        Modifier.size(12.dp).drawBehind {
            val w = 2.dp.toPx()
            drawLine(color, Offset(0f, 0f), Offset(size.width, size.height), w, StrokeCap.Round)
            drawLine(color, Offset(size.width, 0f), Offset(0f, size.height), w, StrokeCap.Round)
        },
    )
}

@Composable
private fun SendGlyph(color: Color) {
    // An arrow that points up.
    Box(
        Modifier.size(18.dp).drawBehind {
            val w = 2.dp.toPx()
            val cx = size.width / 2
            drawLine(color, Offset(cx, size.height), Offset(cx, 1.dp.toPx()), w, StrokeCap.Round)
            val head = Path().apply {
                moveTo(size.width * 0.12f, size.height * 0.46f)
                lineTo(cx, 1.dp.toPx())
                lineTo(size.width * 0.88f, size.height * 0.46f)
            }
            drawPath(head, color, style = Stroke(width = w, cap = StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
        },
    )
}
