package space.foid.chord.ui.theme

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

// Chord design tokens. The values copy chord-desktop/src/lib/theme/tokens.css and the
// themes in chord-desktop/src/lib/theme/themes. Keep the names the same as the CSS names.

/** The colours of one theme. One field per CSS colour token. */
@Immutable
data class ChordColors(
    val surface100: Color,
    val surface200: Color,
    val surface300: Color,
    val surfaceRail: Color,
    val surfaceSide: Color,
    val surfaceRaised: Color,
    val line: Color,
    val lineStrong: Color,
    val ink: Color,
    val inkMuted: Color,
    val brand: Color,
    val brandSoft: Color,
    val onBrand: Color,
    val brandInk: Color,
    val accent: Color,
    val online: Color,
    val away: Color,
    val danger: Color,
    val onDanger: Color,
    val scrim: Color,
    val isDark: Boolean,
) {
    // Interaction overlays: --hover, --press, --selected (ink at 7, 11 and 15 percent).
    val hover: Color get() = ink.copy(alpha = 0.07f)
    val press: Color get() = ink.copy(alpha = 0.11f)
    val selected: Color get() = ink.copy(alpha = 0.15f)
}

/** chord-dark.css. --surface-raised is ink 14% over the rail. */
val ChordDark = ChordColors(
    surface100 = Color(0xFF111316),
    surface200 = Color(0xFF181B20),
    surface300 = Color(0xFF21252B),
    surfaceRail = Color(0xFF323841),
    surfaceSide = Color(0xFF1E2229),
    surfaceRaised = Color(0xFF4C5157),
    line = Color(0xFF2E333B),
    lineStrong = Color(0xFF434A55),
    ink = Color(0xFFECE8E1),
    inkMuted = Color(0xFF9AA0A8),
    brand = Color(0xFFF2A93B),
    brandSoft = Color(0xFF3A2C16),
    onBrand = Color(0xFF1A1C20),
    brandInk = Color(0xFFF2A93B),
    accent = Color(0xFF4CC3B5),
    online = Color(0xFF5BC46E),
    away = Color(0xFFF2A93B),
    danger = Color(0xFFF0676B),
    onDanger = Color(0xFF1A1C20),
    scrim = Color(0x99000000),
    isDark = true,
)

/** chord-light.css. --surface-raised is white 80% over the rail. */
val ChordLight = ChordColors(
    surface100 = Color(0xFFF6F4EF),
    surface200 = Color(0xFFECE9E2),
    surface300 = Color(0xFFE1DDD4),
    surfaceRail = Color(0xFFCAC5B8),
    surfaceSide = Color(0xFFE8E4DA),
    surfaceRaised = Color(0xFFF4F3F1),
    line = Color(0xFFCFCAC0),
    lineStrong = Color(0xFFAAA498),
    ink = Color(0xFF1A1C20),
    inkMuted = Color(0xFF5B6068),
    brand = Color(0xFFC47B0C),
    brandSoft = Color(0xFFFBE7C4),
    onBrand = Color(0xFF1A1C20),
    brandInk = Color(0xFF7A4A00),
    accent = Color(0xFF0A655C),
    online = Color(0xFF2E8B45),
    away = Color(0xFFC47B0C),
    danger = Color(0xFFA82328),
    onDanger = Color(0xFFFFFFFF),
    scrim = Color(0x731A1C20),
    isDark = false,
)

/** Spacing on a 4dp grid: --space-1 to --space-24. */
object ChordSpace {
    val s1 = 4.dp
    val s2 = 8.dp
    val s3 = 12.dp
    val s4 = 16.dp
    val s6 = 24.dp
    val s8 = 32.dp
    val s16 = 64.dp
    val s24 = 96.dp
}

/** --radius-sm, --radius-md, --radius-lg. */
object ChordRadius {
    val sm = 4.dp
    val md = 8.dp
    val lg = 16.dp
}

/** --dur-fast, --dur-arrive, --dur-slow in ms. The ease is ChordEase in ChordTheme.kt. */
object ChordMotion {
    const val FAST = 120
    const val ARRIVE = 180
    const val SLOW = 320
}

/** Mobile sizes. The rail keeps the desktop width. */
object ChordSize {
    val rail = 64.dp
    val bar = 48.dp
    val avatar = 40.dp
    val railIcon = 48.dp
}
