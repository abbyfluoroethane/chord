package space.foid.chord.ui.timeline

import space.foid.chord.ui.components.PresenceBadge
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import space.foid.chord.R
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/*
 * The chrome around the messages of the timeline: header, start of history, date separators,
 * the NEW line, the unread bar, the jump button and the typing line. Every piece is stateless.
 * The wording and the look follow the desktop (ChatHeader.svelte and MessageList.svelte).
 */

/**
 * The 48dp header: menu button, "#" or "@", the name, the presence dot of a 1:1 chat, and the
 * members button. A topic shows as a second line. A tap on it shows all of it, a second tap
 * folds it again.
 *
 * @param topic the room subject, or null/blank for none.
 * @param presence 1:1 chat only. Null shows no dot (the state is not known).
 */
@Composable
fun TimelineHeaderContent(
    title: String,
    isRoom: Boolean,
    onOpenChannels: () -> Unit,
    onOpenMembers: () -> Unit,
    modifier: Modifier = Modifier,
    topic: String? = null,
    presence: Presence? = null,
) {
    val colors = Chord.colors
    var topicOpen by remember(topic) { mutableStateOf(false) }
    Column(modifier.fillMaxWidth().background(colors.surface100).statusBarsPadding()) {
        Row(
            Modifier.fillMaxWidth().defaultMinSize(minHeight = ChordSize.bar),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            HeaderButton(
                stringResource(R.string.timeline_open_channels),
                onOpenChannels,
                Modifier.testTag("drawer_open_channels"),
            ) { MenuGlyph(colors.ink) }
            Column(Modifier.weight(1f).padding(vertical = ChordSpace.s1)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        if (isRoom) "#" else "@",
                        style = ChordType.title,
                        color = colors.inkMuted,
                        modifier = Modifier.testTag("header_glyph"),
                    )
                    Text(
                        title,
                        style = ChordType.title,
                        color = colors.ink,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false).padding(start = ChordSpace.s1),
                    )
                    if (!isRoom && presence != null) {
                        PresenceBadge(presence, Modifier.padding(start = ChordSpace.s2).semantics { contentDescription = presence.label }.testTag("header_presence"))
                    }
                }
                if (!topic.isNullOrBlank()) {
                    Text(
                        topic,
                        style = ChordType.bodySmall,
                        color = colors.inkMuted,
                        maxLines = if (topicOpen) Int.MAX_VALUE else 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier
                            .fillMaxWidth()
                            .semantics {
                                role = Role.Button
                                contentDescription = topic
                            }
                            .clickable { topicOpen = !topicOpen }
                            .testTag("header_topic"),
                    )
                }
            }
            HeaderButton(
                stringResource(if (isRoom) R.string.timeline_open_members else R.string.timeline_open_profile),
                onOpenMembers,
                Modifier.testTag("drawer_open_members"),
            ) { MembersGlyph(colors.ink) }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(colors.line))
    }
}

@Composable
private fun HeaderButton(label: String, onClick: () -> Unit, modifier: Modifier, glyph: @Composable () -> Unit) {
    Box(
        modifier
            .size(ChordSize.bar)
            .semantics { contentDescription = label; role = Role.Button }
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { glyph() }
}

@Composable
private fun MenuGlyph(color: Color) {
    Box(
        Modifier.size(20.dp).drawBehind {
            val w = 2.dp.toPx()
            for (f in listOf(0.2f, 0.5f, 0.8f)) {
                drawLine(color, Offset(0f, size.height * f), Offset(size.width, size.height * f), w, StrokeCap.Round)
            }
        },
    )
}

@Composable
private fun MembersGlyph(color: Color) {
    // A head and shoulders.
    Box(
        Modifier.size(22.dp).drawBehind {
            val w = 2.dp.toPx()
            drawCircle(color, radius = size.width * 0.19f, center = Offset(size.width / 2, size.height * 0.32f), style = Stroke(w))
            drawArc(
                color, startAngle = 180f, sweepAngle = 180f, useCenter = false,
                topLeft = Offset(size.width * 0.12f, size.height * 0.58f),
                size = Size(size.width * 0.76f, size.height * 0.7f),
                style = Stroke(w, cap = StrokeCap.Round),
            )
        },
    )
}

/** The top of the history: "Welcome to #general" or the name of the person, with one line below. */
@Composable
fun StartOfHistory(title: String, isRoom: Boolean, modifier: Modifier = Modifier) {
    val colors = Chord.colors
    val text = startText(title, isRoom)
    Column(
        modifier.fillMaxWidth().padding(start = ChordSpace.s4, end = ChordSpace.s4, top = ChordSpace.s8, bottom = ChordSpace.s2)
            .testTag("start_of_history"),
    ) {
        Text(
            text.title,
            style = ChordType.title.copy(fontSize = 28.sp, lineHeight = 32.sp),
            color = colors.ink,
        )
        Text(text.subtitle, style = ChordType.body, color = colors.inkMuted, modifier = Modifier.padding(top = ChordSpace.s2))
    }
}

/** A centred day label between two rules: "Saturday, October 3, 2026". */
@Composable
fun DateSeparator(label: String, modifier: Modifier = Modifier) {
    val colors = Chord.colors
    Row(
        modifier
            .fillMaxWidth()
            .padding(start = ChordSpace.s4, end = ChordSpace.s4, top = ChordSpace.s6, bottom = ChordSpace.s2)
            .testTag("date_separator"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) {
        Box(Modifier.weight(1f).height(1.dp).background(colors.lineStrong))
        Text(label, style = ChordType.caption.copy(fontWeight = FontWeight.SemiBold, letterSpacing = 0.02.sp), color = colors.ink)
        Box(Modifier.weight(1f).height(1.dp).background(colors.lineStrong))
    }
}

/** The red "NEW" line above the first unread message. Like the desktop, the rule is on the right. */
@Composable
fun NewDivider(modifier: Modifier = Modifier) {
    val colors = Chord.colors
    val description = stringResource(R.string.timeline_new_divider_description)
    Row(
        modifier
            .fillMaxWidth()
            .padding(start = ChordSpace.s4, end = ChordSpace.s4, top = ChordSpace.s4, bottom = ChordSpace.s1)
            .semantics(mergeDescendants = true) { contentDescription = description }
            .testTag("new_divider"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) {
        Text(
            stringResource(R.string.timeline_new_divider).uppercase(),
            style = ChordType.caption.copy(fontWeight = FontWeight.SemiBold, letterSpacing = 0.02.sp),
            color = colors.danger,
        )
        Box(Modifier.weight(1f).height(1.dp).background(colors.danger))
    }
}

/** "13 new messages since 12:14" with "Mark as read", pinned to the top of the list. */
@Composable
fun UnreadBar(text: String, onMarkRead: () -> Unit, modifier: Modifier = Modifier) {
    val colors = Chord.colors
    val shape = RoundedCornerShape(bottomStart = ChordRadius.md, bottomEnd = ChordRadius.md)
    Row(
        modifier
            .fillMaxWidth()
            .padding(horizontal = ChordSpace.s4)
            .background(colors.surface300, shape)
            .border(1.dp, colors.line, shape)
            .height(36.dp)
            .padding(horizontal = ChordSpace.s3)
            .testTag("unread_bar"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(
            text,
            style = ChordType.bodySmall.copy(fontWeight = FontWeight.Medium),
            color = colors.ink,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        Text(
            stringResource(R.string.timeline_mark_read),
            style = ChordType.bodySmall.copy(fontWeight = FontWeight.SemiBold),
            color = colors.accent,
            modifier = Modifier
                .padding(start = ChordSpace.s3)
                .semantics { role = Role.Button }
                .clickable(onClick = onMarkRead)
                .padding(vertical = ChordSpace.s2)
                .testTag("unread_bar_mark_read"),
        )
    }
}

/** The "Jump to present" pill with a down arrow, for a list that is scrolled up. */
@Composable
fun JumpToPresent(onClick: () -> Unit, modifier: Modifier = Modifier) {
    val colors = Chord.colors
    Row(
        modifier
            .background(colors.brand, CircleShape)
            .clickable(onClick = onClick)
            .padding(start = ChordSpace.s4, end = ChordSpace.s3, top = ChordSpace.s2, bottom = ChordSpace.s2)
            .testTag("jump_to_latest"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s1),
    ) {
        Text(stringResource(R.string.timeline_jump_to_present), style = ChordType.label, color = colors.onBrand)
        val ink = colors.onBrand
        Box(
            Modifier.size(14.dp).drawBehind {
                val w = 2.dp.toPx()
                val path = Path().apply {
                    moveTo(size.width / 2, size.height * 0.1f)
                    lineTo(size.width / 2, size.height * 0.9f)
                    moveTo(size.width * 0.15f, size.height * 0.55f)
                    lineTo(size.width / 2, size.height * 0.9f)
                    lineTo(size.width * 0.85f, size.height * 0.55f)
                }
                drawPath(path, ink, style = Stroke(w, cap = StrokeCap.Round, join = StrokeJoin.Round))
            },
        )
    }
}

/**
 * The typing line, "Bay is typing…" with three dots. It keeps its height when nobody types, so
 * the composer does not jump. [text] is empty for nobody.
 */
@Composable
fun TypingLine(text: String, modifier: Modifier = Modifier) {
    val colors = Chord.colors
    val description = stringResource(R.string.timeline_typing_description)
    Row(
        modifier
            .fillMaxWidth()
            .height(20.dp)
            .padding(horizontal = ChordSpace.s4)
            .testTag("typing_line"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (text.isNotEmpty()) {
            val dot = colors.inkMuted
            Box(
                Modifier.size(width = 16.dp, height = 4.dp).drawBehind {
                    for (i in 0..2) drawCircle(dot.copy(alpha = 0.4f + 0.3f * i), 2.dp.toPx(), Offset((2 + 6 * i).dp.toPx(), size.height / 2))
                },
            )
            Text(
                text,
                style = ChordType.caption,
                color = colors.inkMuted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.semantics { contentDescription = "$description: $text" },
            )
        }
    }
}
