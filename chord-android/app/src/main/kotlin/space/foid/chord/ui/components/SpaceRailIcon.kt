package space.foid.chord.ui.components

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordEase
import space.foid.chord.ui.theme.ChordMotion
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordType

/** The kind of a space rail icon. */
enum class RailIconKind { Space, Home }

/**
 * One slot on the space rail (RailItem.svelte): the left pill, a tile and a mention badge.
 * The slot is as wide as the rail ([ChordSize.rail]) and 52dp high. It draws no background:
 * the rail gives the colour behind it. Pass that colour as [cut], for the ring of the badge.
 *
 * At rest a Space tile is a circle that shows the picture, or the initials on a tint from [seed].
 * Selected, it becomes a rounded square and the pill grows to full height. With unread
 * messages (and not selected) the pill is a short stub.
 *
 * @param name name of the space. The initials and the accessibility label come from it.
 * @param seed seed of the fallback tint. Defaults to [name].
 * @param unread number of unread messages. Any value above zero shows the stub.
 * @param mentions number of mentions. Shows the badge.
 */
@Composable
fun SpaceRailIcon(
    name: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: RailIconKind = RailIconKind.Space,
    image: ImageBitmap? = null,
    seed: String = name,
    unread: Int = 0,
    mentions: Int = 0,
    cut: Color = Chord.colors.surfaceRail,
) {
    val c = Chord.colors
    val hasUnread = unread > 0 || mentions > 0
    val dur = tween<Float>(ChordMotion.FAST, easing = ChordEase)
    val pill by animateFloatAsState(
        when {
            selected -> 1f
            hasUnread -> 0.22f
            else -> 0f
        },
        dur, label = "pill",
    )
    val corner by animateDpAsState(
        if (selected) ChordRadius.lg else 22.dp,
        tween(ChordMotion.FAST, easing = ChordEase), label = "corner",
    )
    val tileSize by animateDpAsState(
        if (selected) 40.dp else 44.dp,
        tween(ChordMotion.FAST, easing = ChordEase), label = "tile",
    )
    val desc = name +
        (if (mentions > 0) ", $mentions ${if (mentions == 1) "mention" else "mentions"}" else "") +
        (if (mentions == 0 && unread > 0) ", unread" else "")

    Box(
        modifier
            .width(ChordSize.rail)
            .height(52.dp)
            .semantics(mergeDescendants = true) {
                contentDescription = desc
                this.selected = selected
            }
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                role = Role.Tab,
                onClick = onClick,
            ),
        contentAlignment = Alignment.Center,
    ) {
        // The pill, on the left edge. Its height scales from the 36dp full height.
        Box(
            Modifier
                .align(Alignment.CenterStart)
                .width(4.dp)
                .height(36.dp * pill)
                .clip(RoundedCornerShape(topEnd = 2.dp, bottomEnd = 2.dp))
                .background(c.ink),
        )
        val shape = RoundedCornerShape(corner)
        val fill = when (kind) {
            RailIconKind.Home -> if (selected) c.brand else c.surfaceRaised
            RailIconKind.Space -> Color.Transparent
        }
        Box(
            Modifier.size(tileSize).clip(shape).background(fill),
            contentAlignment = Alignment.Center,
        ) {
            when {
                kind == RailIconKind.Home -> HomeGlyph(if (selected) c.onBrand else c.ink)
                image != null -> Image(
                    bitmap = image,
                    contentDescription = null,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.size(tileSize),
                )
                else -> {
                    val tint = remember(seed) { avatarTint(seed) }
                    Box(Modifier.size(tileSize).background(tint), contentAlignment = Alignment.Center) {
                        Text(
                            avatarInitials(name),
                            style = ChordType.label.copy(fontSize = 15.sp, fontWeight = FontWeight.SemiBold),
                            color = Color(0xFFF6F4EF),
                        )
                    }
                }
            }
        }
        if (mentions > 0) {
            CountBadge(
                mentions,
                ring = cut,
                modifier = Modifier.align(Alignment.BottomEnd).padding(end = 4.dp),
            )
        }
    }
}

/**
 * A brand pill with a count, "99+" above 99. With [ring], a border of that colour, 3dp wide,
 * sits inside the 20dp height: it cuts the badge out of what is behind it.
 */
@Composable
fun CountBadge(count: Int, modifier: Modifier = Modifier, ring: Color? = null) {
    val c = Chord.colors
    val rim = if (ring != null) 3.dp else 0.dp
    val pill = RoundedCornerShape(10.dp)
    Box(
        modifier
            .clip(pill)
            .background(ring ?: Color.Transparent)
            .padding(rim),
    ) {
        Box(
            Modifier
                .height(20.dp - rim * 2)
                .widthIn(min = 20.dp - rim * 2)
                .clip(pill)
                .background(c.brand)
                .padding(horizontal = 5.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                if (count > 99) "99+" else count.toString(),
                style = ChordType.caption.copy(fontSize = 11.sp, lineHeight = 14.sp, fontWeight = FontWeight.SemiBold),
                color = c.onBrand,
            )
        }
    }
}

/** The Home glyph: a house outline, 22dp. */
@Composable
private fun HomeGlyph(color: Color) {
    Canvas(Modifier.size(22.dp)) {
        val u = size.width / 24f
        val s = Stroke(2f * u, cap = StrokeCap.Round, join = StrokeJoin.Round)
        val path = Path().apply {
            moveTo(3f * u, 10.5f * u)
            lineTo(12f * u, 3f * u)
            lineTo(21f * u, 10.5f * u)
            lineTo(21f * u, 20f * u)
            lineTo(15f * u, 20f * u)
            lineTo(15f * u, 14f * u)
            lineTo(9f * u, 14f * u)
            lineTo(9f * u, 20f * u)
            lineTo(3f * u, 20f * u)
            close()
        }
        drawPath(path, color, style = s)
    }
}
