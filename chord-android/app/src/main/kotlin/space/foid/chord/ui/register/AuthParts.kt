package space.foid.chord.ui.register

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

// Small parts that the sign-in and the registration share, in the look of the desktop login card.

/** The red error line with the alert icon (desktop: `.error` with `circle-alert`). */
@Composable
fun AuthError(text: String, tag: String, modifier: Modifier = Modifier) {
    val color = Chord.colors.danger
    Row(
        modifier
            .fillMaxWidth()
            .semantics { liveRegion = LiveRegionMode.Polite }
            .testTag(tag),
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) {
        Canvas(Modifier.padding(top = 2.dp).size(16.dp)) {
            val w = 1.5.dp.toPx()
            drawCircle(color, radius = size.minDimension / 2 - w / 2, style = Stroke(w))
            drawLine(color, Offset(size.width / 2, size.height * 0.28f), Offset(size.width / 2, size.height * 0.56f), w, StrokeCap.Round)
            drawCircle(color, radius = w * 0.7f, center = Offset(size.width / 2, size.height * 0.72f))
        }
        Text(text, style = ChordType.bodySmall, color = color)
    }
}

/** The back arrow and "Back" of a step (desktop: `.back`). */
@Composable
fun AuthBack(label: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val color = Chord.colors.inkMuted
    Row(
        modifier
            .heightIn(min = 48.dp)
            .clickable(role = Role.Button, onClick = onClick),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s1),
    ) {
        Canvas(Modifier.size(16.dp)) {
            val w = 1.5.dp.toPx()
            val y = size.height / 2
            drawLine(color, Offset(size.width * 0.9f, y), Offset(size.width * 0.1f, y), w, StrokeCap.Round)
            drawLine(color, Offset(size.width * 0.1f, y), Offset(size.width * 0.5f, y - size.height * 0.4f), w, StrokeCap.Round)
            drawLine(color, Offset(size.width * 0.1f, y), Offset(size.width * 0.5f, y + size.height * 0.4f), w, StrokeCap.Round)
        }
        Text(label, style = ChordType.label, color = color)
    }
}

/** The amber primary button. While [busy] it shows [busyLabel] with a spinner and keeps its colour. */
@Composable
fun AuthButton(
    label: String,
    busyLabel: String,
    busy: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    tag: String,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    Button(
        onClick = onClick,
        enabled = enabled && !busy,
        shape = RoundedCornerShape(ChordRadius.md),
        colors = ButtonDefaults.buttonColors(
            containerColor = c.brand,
            contentColor = c.onBrand,
            disabledContainerColor = if (busy) c.brand else c.surface300,
            disabledContentColor = if (busy) c.onBrand else c.inkMuted,
        ),
        modifier = modifier
            .widthIn(max = 420.dp)
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .testTag(tag),
    ) {
        if (busy) {
            CircularProgressIndicator(Modifier.size(20.dp), color = c.onBrand, strokeWidth = 2.dp)
            Spacer(Modifier.size(ChordSpace.s3))
            Text(busyLabel, style = ChordType.name)
        } else {
            Text(label, style = ChordType.name)
        }
    }
}

/** A quiet text button (desktop: `.adv`, "Create an account"). */
@Composable
fun AuthLink(label: String, onClick: () -> Unit, tag: String, enabled: Boolean = true, color: Color = Chord.colors.inkMuted) {
    Row(
        Modifier
            .heightIn(min = 48.dp)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = ChordType.label, color = if (enabled) color else color.copy(alpha = 0.5f))
    }
}
