package space.foid.chord.ui.sheets

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/** The shape of every sheet: the top corners are round. */
internal val SheetShape = RoundedCornerShape(topStart = ChordRadius.lg, topEnd = ChordRadius.lg)

/**
 * A Material 3 modal sheet with the Chord tokens. [content] gets `dismissThen`: it hides the
 * sheet with the sheet animation, then runs the action and calls [onDismiss].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ChordModalSheet(
    onDismiss: () -> Unit,
    content: @Composable (dismissThen: (() -> Unit) -> Unit) -> Unit,
) {
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val scope = rememberCoroutineScope()
    val dismissThen: (() -> Unit) -> Unit = { action ->
        scope.launch { state.hide() }.invokeOnCompletion {
            action()
            onDismiss()
        }
    }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = state,
        shape = SheetShape,
        containerColor = Chord.colors.surface200,
        contentColor = Chord.colors.ink,
        tonalElevation = 0.dp,
        scrimColor = Chord.colors.scrim,
        dragHandle = { SheetHandle() },
    ) {
        content(dismissThen)
    }
}

/** The grip on top of a sheet. */
@Composable
internal fun SheetHandle() {
    Box(Modifier.fillMaxWidth().padding(top = ChordSpace.s3, bottom = ChordSpace.s2), contentAlignment = Alignment.Center) {
        Box(Modifier.width(36.dp).height(4.dp).clip(RoundedCornerShape(2.dp)).background(Chord.colors.lineStrong))
    }
}

/** The look of a sheet, for the stateless content in screenshot tests. */
@Composable
internal fun SheetFrame(content: @Composable () -> Unit) {
    Box(
        Modifier.fillMaxWidth().clip(SheetShape).background(Chord.colors.surface200),
    ) {
        androidx.compose.foundation.layout.Column {
            SheetHandle()
            content()
        }
    }
}

enum class SheetIcon { Reply, Edit, Copy, Delete, Image, File, Plus, Search, Check }

/** A 24 unit line icon drawn with [tint]. */
@Composable
internal fun LineIcon(icon: SheetIcon, tint: Color, size: Dp = 22.dp) {
    Box(
        Modifier.size(size).drawBehind {
            val k = this.size.width / 24f
            val stroke = Stroke(width = 1.8f * k, cap = StrokeCap.Round, join = StrokeJoin.Round)
            fun path(build: Path.(Float) -> Unit) = Path().apply { build(k) }
            fun line(a: Float, b: Float, c: Float, d: Float) =
                drawLine(tint, Offset(a * k, b * k), Offset(c * k, d * k), strokeWidth = stroke.width, cap = StrokeCap.Round)
            when (icon) {
                SheetIcon.Reply -> {
                    drawPath(path { s -> moveTo(10 * s, 6 * s); lineTo(4 * s, 12 * s); lineTo(10 * s, 18 * s) }, tint, style = stroke)
                    drawPath(
                        path { s -> moveTo(4 * s, 12 * s); lineTo(14 * s, 12 * s); cubicTo(18 * s, 12 * s, 20 * s, 14 * s, 20 * s, 19 * s) },
                        tint, style = stroke,
                    )
                }
                SheetIcon.Edit -> drawPath(
                    path { s ->
                        moveTo(15 * s, 5 * s); lineTo(19 * s, 9 * s); lineTo(8 * s, 20 * s); lineTo(4 * s, 20 * s)
                        lineTo(4 * s, 16 * s); close()
                    },
                    tint, style = stroke,
                )
                SheetIcon.Copy -> {
                    drawRoundRect(tint, Offset(9 * k, 9 * k), Size(11 * k, 11 * k), CornerRadius(2 * k), style = stroke)
                    drawPath(path { s -> moveTo(5 * s, 15 * s); lineTo(5 * s, 6 * s); quadraticTo(5 * s, 4 * s, 7 * s, 4 * s); lineTo(15 * s, 4 * s) }, tint, style = stroke)
                }
                SheetIcon.Delete -> {
                    line(4f, 7f, 20f, 7f)
                    drawPath(path { s -> moveTo(10 * s, 7 * s); lineTo(10 * s, 4 * s); lineTo(14 * s, 4 * s); lineTo(14 * s, 7 * s) }, tint, style = stroke)
                    drawPath(path { s -> moveTo(6 * s, 7 * s); lineTo(7 * s, 20 * s); lineTo(17 * s, 20 * s); lineTo(18 * s, 7 * s) }, tint, style = stroke)
                }
                SheetIcon.Image -> {
                    drawRoundRect(tint, Offset(3 * k, 5 * k), Size(18 * k, 14 * k), CornerRadius(2.5f * k), style = stroke)
                    drawCircle(tint, 1.6f * k, Offset(9 * k, 10 * k), style = stroke)
                    drawPath(path { s -> moveTo(3.5f * s, 17 * s); lineTo(9 * s, 13 * s); lineTo(13 * s, 16.5f * s); lineTo(16 * s, 14 * s); lineTo(20.5f * s, 18 * s) }, tint, style = stroke)
                }
                SheetIcon.File -> {
                    drawPath(path { s -> moveTo(7 * s, 3 * s); lineTo(14 * s, 3 * s); lineTo(19 * s, 8 * s); lineTo(19 * s, 21 * s); lineTo(7 * s, 21 * s); close() }, tint, style = stroke)
                    drawPath(path { s -> moveTo(14 * s, 3 * s); lineTo(14 * s, 8 * s); lineTo(19 * s, 8 * s) }, tint, style = stroke)
                }
                SheetIcon.Plus -> { line(12f, 5f, 12f, 19f); line(5f, 12f, 19f, 12f) }
                SheetIcon.Check -> drawPath(path { s -> moveTo(5 * s, 12.5f * s); lineTo(10 * s, 17.5f * s); lineTo(19 * s, 7 * s) }, tint, style = stroke)
                SheetIcon.Search -> {
                    drawCircle(tint, 6 * k, Offset(11 * k, 11 * k), style = stroke)
                    line(15.5f, 15.5f, 20f, 20f)
                }
            }
        },
    )
}

/** One action row of a sheet: an icon, a title and an optional line below it. */
@Composable
internal fun SheetRow(
    icon: SheetIcon,
    title: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    danger: Boolean = false,
    large: Boolean = false,
) {
    val color = if (danger) Chord.colors.danger else Chord.colors.ink
    Row(
        modifier
            .fillMaxWidth()
            .height(if (large) 72.dp else 52.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (large) {
            Box(
                Modifier.size(44.dp).clip(RoundedCornerShape(ChordRadius.md)).background(Chord.colors.surface300),
                contentAlignment = Alignment.Center,
            ) { LineIcon(icon, if (danger) color else Chord.colors.brandInk, 24.dp) }
        } else {
            LineIcon(icon, color)
        }
        Spacer(Modifier.width(ChordSpace.s4))
        androidx.compose.foundation.layout.Column(Modifier.weight(1f)) {
            Text(title, style = if (large) ChordType.name else ChordType.body, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (subtitle != null) {
                Text(subtitle, style = ChordType.bodySmall, color = Chord.colors.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}
