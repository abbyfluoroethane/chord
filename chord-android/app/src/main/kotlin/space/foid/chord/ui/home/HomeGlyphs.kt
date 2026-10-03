package space.foid.chord.ui.home

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Line glyphs for the Home list and the contacts page. They use a 24 unit grid. */
internal enum class HomeGlyph { People, Search, Back, More, Message, Check, AddUser, Close, Block, Edit, Trash, Copy }

@Composable
internal fun HomeGlyphIcon(glyph: HomeGlyph, color: Color, modifier: Modifier = Modifier, size: Dp = 20.dp) {
    Canvas(modifier.size(size)) {
        val k = this.size.width / 24f
        val st = Stroke(1.9f * k, cap = StrokeCap.Round, join = StrokeJoin.Round)
        fun line(a: Float, b: Float, c: Float, d: Float) =
            drawLine(color, Offset(a * k, b * k), Offset(c * k, d * k), st.width, StrokeCap.Round)
        fun path(build: Path.(Float) -> Unit) = Path().apply { build(k) }
        when (glyph) {
            HomeGlyph.People -> {
                drawCircle(color, 3.6f * k, Offset(9f * k, 8f * k), style = st)
                drawPath(path { s -> moveTo(2.5f * s, 20f * s); cubicTo(2.5f * s, 15f * s, 5.5f * s, 13.5f * s, 9f * s, 13.5f * s); cubicTo(12.5f * s, 13.5f * s, 15.5f * s, 15f * s, 15.5f * s, 20f * s) }, color, style = st)
                drawPath(path { s -> moveTo(15f * s, 4.6f * s); cubicTo(17.6f * s, 5f * s, 18.6f * s, 7f * s, 17.8f * s, 9f * s); cubicTo(17.3f * s, 10.2f * s, 16.4f * s, 10.8f * s, 15.5f * s, 11f * s) }, color, style = st)
                drawPath(path { s -> moveTo(17f * s, 14f * s); cubicTo(19.8f * s, 14.6f * s, 21.5f * s, 16.4f * s, 21.5f * s, 20f * s) }, color, style = st)
            }
            HomeGlyph.Search -> {
                drawCircle(color, 6.5f * k, Offset(10.5f * k, 10.5f * k), style = st)
                line(15.5f, 15.5f, 20.5f, 20.5f)
            }
            HomeGlyph.Back -> {
                line(19f, 12f, 5f, 12f)
                drawPath(path { s -> moveTo(11f * s, 6f * s); lineTo(5f * s, 12f * s); lineTo(11f * s, 18f * s) }, color, style = st)
            }
            HomeGlyph.More -> for (i in -1..1) drawCircle(color, 1.8f * k, Offset(12f * k, (12f + i * 6f) * k))
            HomeGlyph.Message -> drawPath(
                path { s ->
                    moveTo(5f * s, 5f * s); lineTo(19f * s, 5f * s); cubicTo(20.1f * s, 5f * s, 21f * s, 5.9f * s, 21f * s, 7f * s)
                    lineTo(21f * s, 15f * s); cubicTo(21f * s, 16.1f * s, 20.1f * s, 17f * s, 19f * s, 17f * s)
                    lineTo(10f * s, 17f * s); lineTo(5f * s, 21f * s); lineTo(5f * s, 17f * s)
                    cubicTo(3.9f * s, 17f * s, 3f * s, 16.1f * s, 3f * s, 15f * s); lineTo(3f * s, 7f * s)
                    cubicTo(3f * s, 5.9f * s, 3.9f * s, 5f * s, 5f * s, 5f * s); close()
                },
                color, style = st,
            )
            HomeGlyph.Check -> drawPath(path { s -> moveTo(5f * s, 12.5f * s); lineTo(10f * s, 17.5f * s); lineTo(19f * s, 7f * s) }, color, style = st)
            HomeGlyph.AddUser -> {
                drawCircle(color, 3.8f * k, Offset(9f * k, 8f * k), style = st)
                drawPath(path { s -> moveTo(2.5f * s, 20f * s); cubicTo(2.5f * s, 15f * s, 5.5f * s, 13.5f * s, 9f * s, 13.5f * s); cubicTo(11f * s, 13.5f * s, 12.5f * s, 14f * s, 13.5f * s, 15f * s) }, color, style = st)
                line(18f, 12f, 18f, 20f)
                line(14f, 16f, 22f, 16f)
            }
            HomeGlyph.Close -> { line(6f, 6f, 18f, 18f); line(18f, 6f, 6f, 18f) }
            HomeGlyph.Edit -> drawPath(
                path { s ->
                    moveTo(15f * s, 5f * s); lineTo(19f * s, 9f * s); lineTo(8f * s, 20f * s); lineTo(4f * s, 20f * s)
                    lineTo(4f * s, 16f * s); close()
                },
                color, style = st,
            )
            HomeGlyph.Trash -> {
                line(4f, 7f, 20f, 7f)
                drawPath(path { s -> moveTo(10f * s, 7f * s); lineTo(10f * s, 4f * s); lineTo(14f * s, 4f * s); lineTo(14f * s, 7f * s) }, color, style = st)
                drawPath(path { s -> moveTo(6f * s, 7f * s); lineTo(7f * s, 20f * s); lineTo(17f * s, 20f * s); lineTo(18f * s, 7f * s) }, color, style = st)
            }
            HomeGlyph.Copy -> {
                drawRoundRect(color, Offset(9f * k, 9f * k), androidx.compose.ui.geometry.Size(11f * k, 11f * k), androidx.compose.ui.geometry.CornerRadius(2f * k), style = st)
                drawPath(path { s -> moveTo(5f * s, 15f * s); lineTo(5f * s, 6f * s); quadraticTo(5f * s, 4f * s, 7f * s, 4f * s); lineTo(15f * s, 4f * s) }, color, style = st)
            }
            HomeGlyph.Block -> {
                drawCircle(color, 8.5f * k, Offset(12f * k, 12f * k), style = st)
                line(6f, 6f, 18f, 18f)
            }
        }
    }
}
