package space.foid.chord.ui.spaces

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/** The line icons of the space menus. */
enum class SpaceIcon(internal val paths: List<String>) {
    Invite(
        listOf(
            "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2",
            "M9 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8",
            "M19 8v6", "M22 11h-6",
        ),
    ),
    Settings(
        listOf(
            "M21 4h-7", "M10 4H3", "M21 12h-9", "M8 12H3", "M21 20h-5", "M12 20H3",
            "M14 2v4", "M8 10v4", "M16 18v4",
        ),
    ),
    Hash(listOf("M4 9h16", "M4 15h16", "M10 3 8 21", "M16 3l-2 18")),
    Bell(listOf("M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9", "M10.3 21a1.94 1.94 0 0 0 3.4 0")),
    BellOff(
        listOf(
            "M8.7 3A6 6 0 0 1 18 8a21.3 21.3 0 0 0 .6 5", "M17 17H3s3-2 3-9a4.67 4.67 0 0 1 .3-1.7",
            "M10.3 21a1.94 1.94 0 0 0 3.4 0", "m2 2 20 20",
        ),
    ),
    Pencil(listOf("M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z")),
    Exit(listOf("M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4", "m16 17 5-5-5-5", "M21 12H9")),
    CheckAll(listOf("M18 6 7 17l-5-5", "m22 10-7.5 7.5L13 16")),
    Link(
        listOf(
            "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71",
            "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
        ),
    ),
    Topic(listOf("M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z", "M13 8H7", "M17 12H7")),
    Plus(listOf("M12 5v14", "M5 12h14")),
    Check(listOf("M20 6 9 17l-5-5")),
    Chevron(listOf("m9 18 6-6-6-6")),
    Back(listOf("m15 18-6-6 6-6", "M19 12H5")),
    Close(listOf("M18 6 6 18", "m6 6 12 12")),
    Search(listOf("M11 3a8 8 0 1 0 0 16 8 8 0 0 0 0-16", "m21 21-4.3-4.3")),
    Clock(listOf("M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18", "M12 7v5l3 2")),
    Image(listOf("M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z", "M9 8a1 1 0 1 0 0 2 1 1 0 0 0 0-2", "m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21")),
    Copy(listOf("M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2z", "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2")),
    Trash(listOf("M3 6h18", "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2")),
}

/** A 24 unit line icon of the space menus, drawn with [tint]. */
@Composable
fun SpaceGlyph(icon: SpaceIcon, tint: Color, modifier: Modifier = Modifier, size: Dp = 22.dp) {
    val parsed = remember(icon) { icon.paths.map { PathParser().parsePathString(it).toPath() } }
    Box(
        modifier.size(size).drawBehind {
            val k = this.size.width / 24f
            scale(k, k, pivot = androidx.compose.ui.geometry.Offset.Zero) {
                val stroke = Stroke(width = 1.9f, cap = StrokeCap.Round, join = StrokeJoin.Round)
                parsed.forEach { drawPath(it, tint, style = stroke) }
            }
        },
    )
}

/** One action row of a space sheet: an icon, a title, an optional second line and an optional chevron. */
@Composable
fun SpaceSheetRow(
    icon: SpaceIcon,
    title: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    danger: Boolean = false,
    enabled: Boolean = true,
    chevron: Boolean = false,
) {
    val c = Chord.colors
    val color = when {
        !enabled -> c.inkMuted
        danger -> c.danger
        else -> c.ink
    }
    Row(
        modifier
            .fillMaxWidth()
            .height(if (subtitle != null) 60.dp else 52.dp)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SpaceGlyph(icon, if (danger) color else if (enabled) c.ink else c.inkMuted)
        Spacer(Modifier.width(ChordSpace.s4))
        Column(Modifier.weight(1f)) {
            Text(title, style = ChordType.body, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (subtitle != null) {
                Text(subtitle, style = ChordType.bodySmall, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        if (chevron) SpaceGlyph(SpaceIcon.Chevron, c.inkMuted, size = 18.dp)
    }
}

/** The small grey title over a group of rows. */
@Composable
fun SheetSectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(
        text.uppercase(),
        style = ChordType.caption, color = Chord.colors.inkMuted,
        modifier = modifier.padding(start = ChordSpace.s4, top = ChordSpace.s3, bottom = ChordSpace.s1),
    )
}

/** A thin line between groups of rows. */
@Composable
fun SheetDivider() {
    Box(Modifier.fillMaxWidth().height(1.dp).background(Chord.colors.line))
}

/** A rounded tile that shows the first letters of a name. The look of a space on the rail. */
@Composable
fun SpaceTile(name: String, seed: String, size: Dp = 40.dp, image: androidx.compose.ui.graphics.ImageBitmap? = null) {
    val tint = remember(seed) { space.foid.chord.ui.components.avatarTint(seed) }
    Box(
        Modifier.size(size).clip(RoundedCornerShape(ChordRadius.md)).background(tint),
        contentAlignment = Alignment.Center,
    ) {
        if (image != null) {
            androidx.compose.foundation.Image(
                bitmap = image, contentDescription = null,
                contentScale = androidx.compose.ui.layout.ContentScale.Crop,
                modifier = Modifier.size(size),
            )
        } else {
            Text(
                space.foid.chord.ui.components.avatarInitials(name),
                style = ChordType.label, color = Color(0xFFF6F4EF),
            )
        }
    }
}
