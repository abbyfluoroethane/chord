package space.foid.chord.ui.join

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import space.foid.chord.R
import space.foid.chord.ui.components.CountBadge
import space.foid.chord.ui.theme.Chord

/**
 * The two buttons in the header of the channel drawer: the inbox, with a badge for the number
 * of requests and invitations, and "+" for a new conversation.
 */
@Composable
fun DrawerHeaderActions(
    inboxCount: Int,
    onInbox: () -> Unit,
    onNew: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    Row(modifier, verticalAlignment = Alignment.CenterVertically) {
        val inboxDesc = if (inboxCount > 0) {
            pluralStringResource(R.plurals.inbox_button_count, inboxCount, inboxCount)
        } else {
            stringResource(R.string.inbox_button)
        }
        Box(
            Modifier.size(44.dp).clip(CircleShape)
                .clickable(role = Role.Button, onClick = onInbox)
                .semantics { contentDescription = inboxDesc }
                .testTag("inbox_button"),
            contentAlignment = Alignment.Center,
        ) {
            Glyph(InboxGlyph.Tray, c.ink)
            if (inboxCount > 0) {
                CountBadge(
                    inboxCount, ring = c.surfaceSide,
                    modifier = Modifier.align(Alignment.TopEnd).offset(x = (-2).dp, y = 2.dp).testTag("inbox_badge"),
                )
            }
        }
        val newDesc = stringResource(R.string.join_button)
        Box(
            Modifier.size(44.dp).clip(CircleShape)
                .clickable(role = Role.Button, onClick = onNew)
                .semantics { contentDescription = newDesc }
                .testTag("new_conversation_button"),
            contentAlignment = Alignment.Center,
        ) { Glyph(InboxGlyph.Plus, c.ink) }
    }
}

internal enum class InboxGlyph { Tray, Plus }

@Composable
private fun Glyph(glyph: InboxGlyph, tint: Color) {
    Box(
        Modifier.size(24.dp).drawBehind {
            val k = size.width / 24f
            val stroke = Stroke(1.9f * k, cap = StrokeCap.Round, join = StrokeJoin.Round)
            when (glyph) {
                InboxGlyph.Plus -> {
                    drawLine(tint, Offset(12 * k, 5 * k), Offset(12 * k, 19 * k), 1.9f * k, StrokeCap.Round)
                    drawLine(tint, Offset(5 * k, 12 * k), Offset(19 * k, 12 * k), 1.9f * k, StrokeCap.Round)
                }
                InboxGlyph.Tray -> {
                    val p = Path().apply {
                        moveTo(3.5f * k, 13 * k); lineTo(6.5f * k, 5 * k); lineTo(17.5f * k, 5 * k); lineTo(20.5f * k, 13 * k)
                        lineTo(20.5f * k, 18 * k); lineTo(3.5f * k, 18 * k); close()
                    }
                    drawPath(p, tint, style = stroke)
                    val q = Path().apply {
                        moveTo(3.5f * k, 13 * k); lineTo(8.5f * k, 13 * k); lineTo(10 * k, 15.5f * k)
                        lineTo(14 * k, 15.5f * k); lineTo(15.5f * k, 13 * k); lineTo(20.5f * k, 13 * k)
                    }
                    drawPath(q, tint, style = stroke)
                }
            }
        },
    )
}
