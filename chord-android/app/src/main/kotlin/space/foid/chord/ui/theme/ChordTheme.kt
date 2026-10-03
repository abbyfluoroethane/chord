package space.foid.chord.ui.theme

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

/** --ease-out: cubic-bezier(0.2, 0, 0, 1). */
val ChordEase = CubicBezierEasing(0.2f, 0f, 0f, 1f)

/**
 * Type styles. The desktop uses IBM Plex Sans, IBM Plex Mono and Bricolage Grotesque.
 * TODO: bundle those fonts as res/font. Until then the system fonts stand in.
 */
object ChordType {
    val sans: FontFamily = FontFamily.SansSerif
    val mono: FontFamily = FontFamily.Monospace
    val display: FontFamily = FontFamily.SansSerif

    val body = TextStyle(fontFamily = sans, fontSize = 15.sp, lineHeight = 22.sp)
    val bodySmall = TextStyle(fontFamily = sans, fontSize = 13.sp, lineHeight = 18.sp)
    val label = TextStyle(fontFamily = sans, fontSize = 14.sp, lineHeight = 20.sp, fontWeight = FontWeight.Medium)
    val name = TextStyle(fontFamily = sans, fontSize = 15.sp, lineHeight = 20.sp, fontWeight = FontWeight.SemiBold)
    val caption = TextStyle(fontFamily = sans, fontSize = 12.sp, lineHeight = 16.sp)
    val title = TextStyle(fontFamily = display, fontSize = 20.sp, lineHeight = 26.sp, fontWeight = FontWeight.Bold)
    val code = TextStyle(fontFamily = mono, fontSize = 14.sp, lineHeight = 20.sp)
}

private val LocalChordColors = staticCompositionLocalOf { ChordDark }

/** Access the tokens: `Chord.colors.ink`. */
object Chord {
    val colors: ChordColors
        @Composable @ReadOnlyComposable get() = LocalChordColors.current
}

/**
 * The Chord theme. Material 3 gets a colour scheme mapped from the tokens, so the Material
 * plumbing (sheets, dialogs, text fields, navigation) uses Chord colours.
 */
@Composable
fun ChordTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val c = if (dark) ChordDark else ChordLight
    val scheme = if (dark) {
        darkColorScheme(
            primary = c.brand, onPrimary = c.onBrand, primaryContainer = c.brandSoft, onPrimaryContainer = c.brandInk,
            secondary = c.accent, onSecondary = c.surface100,
            background = c.surface100, onBackground = c.ink,
            surface = c.surface200, onSurface = c.ink, surfaceVariant = c.surface300, onSurfaceVariant = c.inkMuted,
            surfaceContainer = c.surface200, surfaceContainerHigh = c.surface300, surfaceContainerLow = c.surface100,
            outline = c.lineStrong, outlineVariant = c.line,
            error = c.danger, onError = c.onDanger, scrim = c.scrim,
        )
    } else {
        lightColorScheme(
            primary = c.brand, onPrimary = c.onBrand, primaryContainer = c.brandSoft, onPrimaryContainer = c.brandInk,
            secondary = c.accent, onSecondary = c.surface100,
            background = c.surface100, onBackground = c.ink,
            surface = c.surface200, onSurface = c.ink, surfaceVariant = c.surface300, onSurfaceVariant = c.inkMuted,
            surfaceContainer = c.surface200, surfaceContainerHigh = c.surface300, surfaceContainerLow = c.surface100,
            outline = c.lineStrong, outlineVariant = c.line,
            error = c.danger, onError = c.onDanger, scrim = c.scrim,
        )
    }
    CompositionLocalProvider(LocalChordColors provides c) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
