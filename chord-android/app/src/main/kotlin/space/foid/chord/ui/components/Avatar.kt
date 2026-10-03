package space.foid.chord.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordType
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * Presence of a contact. The shapes follow the desktop (Presence.svelte): colour and shape
 * together, so the state does not depend on colour alone. Offline is a grey ring.
 * Pass `null` as presence where the state is unknown: then no mark shows.
 */
enum class Presence(val label: String) {
    Online("Online"),
    Away("Away"),
    Dnd("Do not disturb"),
    Offline("Offline"),
}

/** The tint hues. They skip the purple range on purpose, as the desktop does (format.ts). */
private val Hues = intArrayOf(12, 32, 52, 95, 150, 175, 195, 215, 345)

/** A stable background colour for a JID or a name. The same seed gives the same colour. */
fun avatarTint(seed: String): Color {
    var h = 0L
    for (ch in seed) h = (h * 31 + ch.code) and 0xFFFFFFFFL
    return Color.hsl(Hues[(h % Hues.size).toInt()].toFloat(), 0.42f, 0.30f)
}

/** One or two capital letters from a name, as the desktop shows them. */
fun avatarInitials(name: String): String {
    val parts = name.trim().split(Regex("[\\s-]+")).filter { it.isNotEmpty() }
    return when (parts.size) {
        0 -> "?"
        1 -> parts[0].take(1).uppercase()
        else -> (parts[0].take(1) + parts[1].take(1)).uppercase()
    }
}

private val AvatarInk = Color(0xFFF6F4EF)

/**
 * Avatar with an optional presence mark at the bottom right.
 *
 * @param jid seed of the fallback colour (stable for one contact).
 * @param name text of the initials. Falls back to [jid].
 * @param image the picture, or null for the initials.
 * @param cut colour of the cut-out ring around the mark. Pass the colour that is behind the avatar.
 */
@Composable
fun Avatar(
    jid: String,
    modifier: Modifier = Modifier,
    name: String = jid,
    image: ImageBitmap? = null,
    size: Dp = ChordSize.avatar,
    presence: Presence? = null,
    cut: Color = Chord.colors.surface100,
) {
    Box(modifier.size(size)) {
        if (image != null) {
            Image(
                bitmap = image,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.size(size).clip(CircleShape),
            )
        } else {
            val tint = remember(jid) { avatarTint(jid) }
            Box(Modifier.size(size).clip(CircleShape).background(tint), contentAlignment = Alignment.Center) {
                Text(
                    avatarInitials(name),
                    style = ChordType.label.copy(
                        fontSize = (size.value * 0.4f).sp,
                        lineHeight = (size.value * 0.4f).sp,
                        fontWeight = FontWeight.SemiBold,
                    ),
                    color = AvatarInk,
                )
            }
        }
        if (presence != null) {
            val dot = max(10, (size.value * 0.36f).roundToInt()).dp
            val pad = if (size >= 40.dp) 3.dp else 2.dp
            PresenceMark(
                presence,
                size = dot,
                cut = cut,
                pad = pad,
                // The mark sits at the bottom right, a little past the edge, as on the desktop.
                modifier = Modifier.align(Alignment.BottomEnd).offset(4.dp, 4.dp),
            )
        }
    }
}

/**
 * The presence shape alone. With [cut], a backdrop of that colour, [pad] wide, follows the
 * shape, so it cuts the shape out of what is behind it.
 */
@Composable
fun PresenceMark(
    presence: Presence,
    modifier: Modifier = Modifier,
    size: Dp = 10.dp,
    cut: Color? = null,
    pad: Dp = 0.dp,
) {
    val colors = Chord.colors
    val box = if (cut != null) size + pad * 2 else size
    Canvas(modifier.size(box).semantics { contentDescription = presence.label }) {
        // The shape is 10 units wide, as in the SVG on the desktop.
        val u = size.toPx() / 10f
        val p = if (cut != null) pad.toPx() else 0f
        val o = Offset(p, p)
        if (cut != null) {
            if (presence == Presence.Dnd) {
                drawRoundRect(
                    cut,
                    topLeft = Offset(0f, 3 * u),
                    size = Size(10 * u + 2 * p, 4 * u + 2 * p),
                    cornerRadius = CornerRadius(2 * u + p),
                )
            } else {
                drawCircle(cut, radius = 5 * u + p, center = Offset(5 * u + p, 5 * u + p))
            }
        }
        when (presence) {
            Presence.Online -> drawCircle(colors.online, 5 * u, Offset(5 * u, 5 * u) + o)
            Presence.Away -> {
                val half = Path().apply {
                    arcTo(Rect(o, Size(10 * u, 10 * u)), 270f, -180f, false)
                    close()
                }
                drawPath(half, colors.away, style = Fill)
                drawCircle(colors.away, 4 * u, Offset(5 * u, 5 * u) + o, style = Stroke(2 * u))
            }
            Presence.Dnd -> drawRoundRect(
                colors.danger,
                topLeft = Offset(0f, 3 * u) + o,
                size = Size(10 * u, 4 * u),
                cornerRadius = CornerRadius(2 * u),
            )
            Presence.Offline -> drawCircle(colors.inkMuted, 4 * u, Offset(5 * u, 5 * u) + o, style = Stroke(2 * u))
        }
    }
}
