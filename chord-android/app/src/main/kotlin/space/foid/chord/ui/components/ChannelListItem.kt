package space.foid.chord.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/** What a channel list row stands for. */
enum class ChannelKind {
    /** A room in a space. Shows a hash. The badge counts mentions. */
    Room,

    /** A one-to-one chat. Shows an avatar with presence. The badge counts unread messages. */
    Dm,

    /** A group chat outside a space. Shows an avatar. The badge counts unread messages. */
    Group,
}

/**
 * One row of the channel list (ChannelRow.svelte), at touch size: 44dp for a room, 56dp for a
 * chat that shows an avatar.
 *
 * @param unread number of unread messages. Above zero (and not muted, not selected) the name
 * is bold and a pill shows on the left edge.
 * @param mentions number of mentions. For a room the badge shows this. For a DM or group the
 * badge shows [unread].
 * @param muted dims the row, shows a bell-off mark, and removes the unread state and the badge.
 * @param subtitle second line, for example the member line of a group chat.
 * @param presence DM only. Null shows no mark (the state is not known).
 * @param background colour behind the row, for the cut-out ring of the presence mark.
 * @param onLongClick a long press on the row. Null for none.
 */
@Composable
fun ChannelListItem(
    name: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ChannelKind = ChannelKind.Room,
    jid: String = name,
    image: ImageBitmap? = null,
    presence: Presence? = null,
    unread: Int = 0,
    mentions: Int = 0,
    muted: Boolean = false,
    subtitle: String? = null,
    background: Color = Chord.colors.surfaceSide,
    onLongClick: (() -> Unit)? = null,
) {
    val c = Chord.colors
    val chat = kind != ChannelKind.Room
    val isUnread = unread > 0 && !muted
    val count = if (chat) unread else mentions
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val overlay = when {
        selected -> c.selected
        pressed -> c.press
        else -> Color.Transparent
    }
    val offline = kind == ChannelKind.Dm && presence == Presence.Offline
    val ink = if (selected || isUnread) c.ink else c.inkMuted
    val weight = if (selected || isUnread) FontWeight.SemiBold else FontWeight.Medium
    val desc = (if (chat) "" else "#") + name +
        (if (subtitle != null) ", $subtitle" else "") +
        (if (muted) ", muted" else "") +
        (if (count > 0 && !muted) ", $count ${if (chat) "unread" else if (count == 1) "mention" else "mentions"}"
        else if (isUnread) ", unread" else "")

    Box(
        modifier
            .fillMaxWidth()
            .padding(horizontal = ChordSpace.s2, vertical = 1.dp)
            .semantics(mergeDescendants = true) {
                contentDescription = desc
                this.selected = selected
            },
    ) {
        if (isUnread && !selected) {
            Box(
                Modifier
                    .align(Alignment.CenterStart)
                    .width(4.dp)
                    .height(8.dp)
                    .background(c.ink, RoundedCornerShape(topEnd = ChordRadius.sm, bottomEnd = ChordRadius.sm)),
            )
        }
        Row(
            Modifier
                .fillMaxWidth()
                .height(if (chat) 56.dp else 44.dp)
                .alpha(if (muted) 0.55f else 1f)
                .clip(RoundedCornerShape(ChordRadius.sm))
                .background(overlay)
                .combinedClickable(interactionSource = interaction, indication = null, role = Role.Button, onLongClick = onLongClick, onClick = onClick)
                .padding(horizontal = ChordSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
        ) {
            when (kind) {
                ChannelKind.Dm -> Avatar(
                    jid = jid, name = name, image = image, size = 36.dp, presence = presence,
                    cut = if (selected) c.selected.compositeOver(background) else background,
                )
                ChannelKind.Group -> Avatar(jid = jid, name = name, image = image, size = 36.dp)
                ChannelKind.Room -> HashGlyph(c.inkMuted)
            }
            if (subtitle != null) {
                Column(Modifier.weight(1f)) {
                    Text(
                        name, style = ChordType.body.copy(fontWeight = weight, lineHeight = 20.sp), color = ink,
                        maxLines = 1, overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.alpha(if (offline) 0.7f else 1f),
                    )
                    Text(
                        subtitle, style = ChordType.caption, color = if (selected) c.ink else c.inkMuted,
                        maxLines = 1, overflow = TextOverflow.Ellipsis,
                    )
                }
            } else {
                Text(
                    name, style = ChordType.body.copy(fontWeight = weight, lineHeight = 20.sp), color = ink,
                    maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).alpha(if (offline) 0.7f else 1f),
                )
            }
            if (muted) {
                BellOffGlyph(c.inkMuted)
            } else if (count > 0) {
                CountBadge(count)
            }
        }
    }
}

/** The hash of a room, 20dp. */
@Composable
private fun HashGlyph(color: Color) {
    Canvas(Modifier.size(20.dp)) {
        val u = size.width / 24f
        val st = Stroke(2f * u, cap = StrokeCap.Round)
        drawLine(color, Offset(4f * u, 9f * u), Offset(20f * u, 9f * u), st.width, StrokeCap.Round)
        drawLine(color, Offset(4f * u, 15f * u), Offset(20f * u, 15f * u), st.width, StrokeCap.Round)
        drawLine(color, Offset(10f * u, 3f * u), Offset(8f * u, 21f * u), st.width, StrokeCap.Round)
        drawLine(color, Offset(16f * u, 3f * u), Offset(14f * u, 21f * u), st.width, StrokeCap.Round)
    }
}

/** A bell with a slash, 16dp. */
@Composable
private fun BellOffGlyph(color: Color) {
    Canvas(Modifier.size(16.dp)) {
        val u = size.width / 24f
        val st = Stroke(2f * u, cap = StrokeCap.Round, join = StrokeJoin.Round)
        val bell = Path().apply {
            moveTo(6.5f * u, 8f * u)
            cubicTo(6.5f * u, 4.5f * u, 9f * u, 3f * u, 12f * u, 3f * u)
            cubicTo(14.5f * u, 3f * u, 16.5f * u, 4.2f * u, 17.3f * u, 6.3f * u)
            moveTo(6.5f * u, 8f * u)
            cubicTo(6.5f * u, 14f * u, 4f * u, 16f * u, 4f * u, 16f * u)
            lineTo(14f * u, 16f * u)
            moveTo(17.8f * u, 10f * u)
            cubicTo(18f * u, 14f * u, 20f * u, 16f * u, 20f * u, 16f * u)
            lineTo(18f * u, 16f * u)
            moveTo(10f * u, 20f * u)
            cubicTo(10.6f * u, 21f * u, 13.4f * u, 21f * u, 14f * u, 20f * u)
        }
        drawPath(bell, color, style = st)
        drawLine(color, Offset(3f * u, 3f * u), Offset(21f * u, 21f * u), st.width, StrokeCap.Round)
    }
}
