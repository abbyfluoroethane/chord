package space.foid.chord.ui.components

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.ui.timeline.MessageUi
import space.foid.chord.ui.timeline.ReactionUi
import space.foid.chord.ui.timeline.ReplyUi
import space.foid.chord.ui.timeline.SendState

/**
 * One message of the timeline, cozy layout (Discord mobile). All parameters are stable.
 *
 * @param grouped true for a continuation row: it drops avatar, name and time. The time shows in
 *   the gutter while the row is pressed. Decide it with `continuesGroup`.
 * @param avatar the avatar slot, 40dp ([ChordSize.avatar]). It is not composed for a continuation.
 * @param onLongPress opens the message actions.
 * @param onReactionClick toggles one reaction: gets the emoji.
 * @param onReplyPreviewClick jumps to the quoted message: gets its id, or null if unknown.
 * @param onRetryClick the "Try again" link of a failed message.
 */
@OptIn(ExperimentalFoundationApi::class, ExperimentalLayoutApi::class)
@Composable
fun MessageRow(
    message: MessageUi,
    grouped: Boolean,
    avatar: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    onLongPress: () -> Unit = {},
    onReactionClick: (String) -> Unit = {},
    onReplyPreviewClick: (String?) -> Unit = {},
    onRetryClick: () -> Unit = {},
) {
    val colors = Chord.colors
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val gutter = ChordSize.avatar
    val reply = message.reply
    Column(
        modifier
            .fillMaxWidth()
            .background(if (pressed) colors.hover else Color.Transparent)
            .combinedClickable(interactionSource = source, indication = null, onClick = {}, onLongClick = onLongPress)
            .padding(top = if (grouped) 1.dp else ChordSpace.s3, bottom = 1.dp, start = ChordSpace.s4, end = ChordSpace.s4)
            .alpha(if (message.state == SendState.PENDING) 0.6f else 1f),
    ) {
        if (reply != null) {
            ReplyPreview(reply = reply, gutter = gutter, onClick = { onReplyPreviewClick(reply.targetId) })
        }
        Row {
            Box(Modifier.width(gutter), contentAlignment = if (grouped) Alignment.TopEnd else Alignment.TopStart) {
                if (grouped) {
                    if (pressed) Text(message.timeLabel, style = ChordType.caption, color = colors.inkMuted, maxLines = 1)
                } else {
                    avatar()
                }
            }
            Column(Modifier.weight(1f).padding(start = ChordSpace.s3)) {
                if (!grouped) {
                    Row(verticalAlignment = Alignment.Bottom) {
                        Text(
                            message.senderName,
                            style = ChordType.name,
                            color = if (message.outgoing) colors.brandInk else colors.ink,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        Text(
                            message.timeLabel,
                            style = ChordType.caption,
                            color = colors.inkMuted,
                            maxLines = 1,
                            modifier = Modifier.padding(start = ChordSpace.s2),
                        )
                    }
                }
                if (message.retracted) {
                    Text("Message deleted.", style = ChordType.body.copy(fontStyle = FontStyle.Italic), color = colors.inkMuted)
                } else {
                    if (message.body.isNotEmpty()) MessageText(message)
                    if (message.attachment != null) {
                        Text(
                            message.attachment,
                            style = ChordType.bodySmall,
                            color = colors.accent,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.padding(top = ChordSpace.s1),
                        )
                    }
                    if (message.reactions.isNotEmpty()) {
                        FlowRow(
                            Modifier.padding(top = ChordSpace.s1),
                            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s1),
                            verticalArrangement = Arrangement.spacedBy(ChordSpace.s1),
                        ) {
                            for (r in message.reactions) {
                                key(r.emoji) { ReactionChip(r) { onReactionClick(r.emoji) } }
                            }
                        }
                    }
                    if (message.state == SendState.FAILED) {
                        Row(Modifier.padding(top = ChordSpace.s1)) {
                            Text("Not sent. ", style = ChordType.bodySmall, color = colors.danger)
                            Text(
                                "Try again",
                                style = ChordType.bodySmall.copy(textDecoration = TextDecoration.Underline),
                                color = colors.danger,
                                modifier = Modifier.clickable(onClick = onRetryClick),
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MessageText(message: MessageUi) {
    val colors = Chord.colors
    // The edited marker sits at the end of the text, as on the desktop.
    val muted = colors.inkMuted
    val text = remember(message.body, message.edited, muted) {
        buildAnnotatedString {
            append(message.body)
            if (message.edited) {
                pushStyle(SpanStyle(color = muted, fontSize = ChordType.caption.fontSize))
                append(" (edited)")
                pop()
            }
        }
    }
    Text(text, style = ChordType.body, color = colors.ink)
}

@Composable
private fun ReplyPreview(reply: ReplyUi, gutter: Dp, onClick: () -> Unit) {
    val colors = Chord.colors
    val line = colors.line
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .height(24.dp)
            // The connector from the avatar up to the quoted line, as on the desktop.
            .drawBehind {
                val x = gutter.toPx() / 2f
                val y = size.height / 2f
                val r = 8.dp.toPx()
                val path = Path().apply {
                    moveTo(x, size.height + 4.dp.toPx())
                    lineTo(x, y + r)
                    quadraticTo(x, y, x + r, y)
                    lineTo(gutter.toPx() - 2.dp.toPx(), y)
                }
                drawPath(path, line, style = Stroke(width = 2.dp.toPx()))
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            reply.senderName,
            style = ChordType.bodySmall.copy(fontWeight = FontWeight.SemiBold),
            color = colors.inkMuted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(start = gutter + ChordSpace.s3).weight(1f, fill = false),
        )
        Text(
            reply.snippet,
            style = ChordType.bodySmall,
            color = colors.inkMuted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(start = ChordSpace.s2).weight(1f, fill = true),
        )
    }
}

@Composable
private fun ReactionChip(reaction: ReactionUi, onClick: () -> Unit) {
    val colors = Chord.colors
    val shape = RoundedCornerShape(ChordRadius.md)
    Row(
        Modifier
            .height(28.dp)
            .background(if (reaction.mine) colors.brandSoft else colors.surface300, shape)
            .border(1.dp, if (reaction.mine) colors.brand else colors.line, shape)
            .clickable(onClick = onClick)
            .padding(horizontal = ChordSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(reaction.emoji, style = ChordType.bodySmall)
        Text(
            reaction.count.toString(),
            style = ChordType.label.copy(fontSize = ChordType.bodySmall.fontSize),
            color = if (reaction.mine) colors.brandInk else colors.inkMuted,
        )
    }
}
