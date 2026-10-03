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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.ui.composed
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventTimeoutCancellationException
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.UriHandler
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.sp
import space.foid.chord.R
import space.foid.chord.ui.text.TextBlock
import space.foid.chord.ui.text.formatMessage
import space.foid.chord.ui.text.formatPalette
import space.foid.chord.ui.text.isXmppUri
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
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
import space.foid.chord.ui.attachments.AttachmentView
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
 * @param onImageClick opens the viewer for an inline image: gets its URL.
 * @param onProfileClick a tap on the avatar or the name. Null: they do nothing.
 * @param onXmppLink a tap on an xmpp: link in the text: gets the URI. http(s) links open in the browser.
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
    onImageClick: (String) -> Unit = {},
    onXmppLink: (String) -> Unit = {},
    onProfileClick: (() -> Unit)? = null,
) {
    val colors = Chord.colors
    // The row and the text both detect a long press. Whoever fires first wins.
    val lastLong = remember { longArrayOf(0L) }
    val longPress = rememberUpdatedState(onLongPress)
    val fireLongPress = remember {
        {
            val now = System.nanoTime()
            if (now - lastLong[0] > 600_000_000L) {
                lastLong[0] = now
                longPress.value()
            }
        }
    }
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val gutter = ChordSize.avatar
    val reply = message.reply
    Column(
        modifier
            .fillMaxWidth()
            .background(if (pressed) colors.hover else Color.Transparent)
            .combinedClickable(interactionSource = source, indication = null, onClick = {}, onLongClick = fireLongPress)
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
                    Box(if (onProfileClick != null) Modifier.clickable(role = Role.Button, onClick = onProfileClick).testTag("message_avatar") else Modifier) { avatar() }
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
                            modifier = Modifier.weight(1f, fill = false)
                                .then(if (onProfileClick != null) Modifier.clickable(onClick = onProfileClick).testTag("message_name") else Modifier),
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
                    // A message whose body is only the attachment URL shows the attachment alone.
                    if (message.body.isNotEmpty() && message.body.trim() != message.attachment) MessageText(message, onXmppLink, fireLongPress)
                    if (message.attachment != null) {
                        AttachmentView(message.attachment, outgoing = message.outgoing, onImageClick = onImageClick)
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

/**
 * The message text: the blocks of the formatted body. A tap on an http(s) link opens it with
 * [LocalUriHandler]. A tap on an xmpp: link calls [onXmppLink]. The text has no pointer input of
 * its own beyond the link taps, so a long press reaches the row.
 */
@Composable
private fun MessageText(message: MessageUi, onXmppLink: (String) -> Unit, onLongPress: () -> Unit) {
    val colors = Chord.colors
    val formatted = message.formatted ?: remember(message.body, message.senderName, colors) {
        formatMessage(message.body, colors.formatPalette(), actor = message.senderName)
    }
    val uriHandler = LocalUriHandler.current
    val onXmpp by rememberUpdatedState(onXmppLink)
    val routed = remember(uriHandler) {
        object : UriHandler {
            override fun openUri(uri: String) {
                if (isXmppUri(uri)) onXmpp(uri) else uriHandler.openUri(uri)
            }
        }
    }
    val muted = colors.inkMuted
    val edited = if (message.edited) {
        remember(muted) {
            buildAnnotatedString {
                pushStyle(SpanStyle(color = muted, fontSize = ChordType.caption.fontSize))
                append(" (edited)")
                pop()
            }
        }
    } else null
    val base = when {
        formatted.jumbo -> ChordType.body.copy(fontSize = 40.sp, lineHeight = 48.sp)
        formatted.action -> ChordType.body.copy(fontStyle = FontStyle.Italic)
        else -> ChordType.body
    }
    val color = if (formatted.action) colors.inkMuted else colors.ink
    CompositionLocalProvider(LocalUriHandler provides routed) {
        Column(Modifier.longPressBeforeLinks(onLongPress)) {
            TextBlocks(formatted.blocks, base, color, edited)
        }
    }
}

@Composable
private fun TextBlocks(blocks: List<TextBlock>, base: TextStyle, color: Color, suffix: AnnotatedString?) {
    val colors = Chord.colors
    blocks.forEachIndexed { i, block ->
        val last = i == blocks.lastIndex
        when (block) {
            is TextBlock.Paragraph -> {
                val text = if (last && suffix != null) {
                    remember(block, suffix) { buildAnnotatedString { append(block.text); append(suffix) } }
                } else block.text
                Text(text, style = base, color = color)
            }
            is TextBlock.Quote -> Row(Modifier.padding(vertical = 2.dp).height(IntrinsicSize.Min)) {
                Box(Modifier.width(4.dp).fillMaxHeight().background(colors.line, RoundedCornerShape(2.dp)))
                Column(Modifier.padding(start = ChordSpace.s3)) {
                    TextBlocks(block.blocks, base, color, if (last) suffix else null)
                }
            }
            is TextBlock.Code -> {
                val shape = RoundedCornerShape(ChordRadius.sm)
                Text(
                    block.code,
                    style = ChordType.code,
                    color = colors.ink,
                    modifier = Modifier
                        .padding(vertical = 2.dp)
                        .fillMaxWidth()
                        .background(colors.surface300, shape)
                        .border(1.dp, colors.line, shape)
                        .padding(horizontal = ChordSpace.s2, vertical = 6.dp),
                )
            }
        }
        if (last && suffix != null && block !is TextBlock.Paragraph && block !is TextBlock.Quote) {
            Text(suffix, style = base, color = color)
        }
    }
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
        if (reply.snippet.isEmpty()) {
            Text(
                stringResource(R.string.reply_not_loaded),
                style = ChordType.bodySmall.copy(fontStyle = FontStyle.Italic),
                color = colors.inkMuted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(start = ChordSpace.s2).widthIn(max = 220.dp),
            )
        } else {
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

/**
 * Detects a long press in the initial pass, before a link under the finger can take the touch.
 * A tap or a drag passes through, so link taps and scrolling work as before. After a long press
 * the rest of the touch is consumed, so the link does not open when the finger lifts.
 */
private fun Modifier.longPressBeforeLinks(onLongPress: () -> Unit): Modifier = composed {
    val current = rememberUpdatedState(onLongPress)
    pointerInput(Unit) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            var fired = false
            try {
                withTimeout(viewConfiguration.longPressTimeoutMillis) {
                    while (true) {
                        val event = awaitPointerEvent(PointerEventPass.Initial)
                        val change = event.changes.firstOrNull { it.id == down.id } ?: return@withTimeout
                        if (!change.pressed || (change.position - down.position).getDistance() > viewConfiguration.touchSlop) {
                            return@withTimeout
                        }
                    }
                }
            } catch (e: PointerEventTimeoutCancellationException) {
                fired = true
            }
            if (fired) {
                current.value()
                while (true) {
                    val event = awaitPointerEvent(PointerEventPass.Initial)
                    event.changes.forEach { it.consume() }
                    if (event.changes.none { it.pressed }) break
                }
            }
        }
    }
}
