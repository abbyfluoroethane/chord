package space.foid.chord.ui.theme

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import space.foid.chord.ui.settings.AppPrefs

/** 12 or 24 hour clock. [System] follows the phone. */
enum class TimeFormat {
    System, H12, H24;

    /** True for the 24 hour clock, given the phone setting. */
    fun is24(system24: Boolean): Boolean = when (this) {
        System -> system24
        H12 -> false
        H24 -> true
    }

    companion object {
        fun fromName(name: String?): TimeFormat = entries.firstOrNull { it.name == name } ?: System
    }
}

/** Reduce motion. [System] follows the phone (animations turned off in the system settings). */
enum class MotionMode {
    System, Reduce, Full;

    fun reduce(systemReduce: Boolean): Boolean = when (this) {
        System -> systemReduce
        Reduce -> true
        Full -> false
    }

    companion object {
        fun fromName(name: String?): MotionMode = entries.firstOrNull { it.name == name } ?: System
    }
}

/** `id=accent,id=accent`: the accent picked for each theme. Ids and accents hold no `=` or `,`. */
fun encodeAccents(accents: Map<String, String>): String =
    accents.entries.joinToString(",") { "${it.key}=${it.value}" }

fun decodeAccents(text: String?): Map<String, String> {
    if (text.isNullOrEmpty()) return emptyMap()
    return text.split(',').mapNotNull {
        val i = it.indexOf('=')
        if (i <= 0 || i == it.length - 1) null else it.substring(0, i) to it.substring(i + 1)
    }.toMap()
}

/**
 * The look settings that screens read from [LocalAppearance]. The defaults are what the
 * screenshot tests use.
 */
@Immutable
data class Appearance(
    /** Text size in sp. 15 is the design size. */
    val fontSize: Int = 15,
    val is24h: Boolean = true,
    val underlineLinks: Boolean = true,
    val reduceMotion: Boolean = false,
    val jumboEmoji: Boolean = true,
) {
    /** The factor for the text size: 1 at the design size. */
    val fontFactor: Float get() = fontSize / 15f
}

/** The settings of the look, from the stored preferences and the phone state. */
fun appearanceOf(prefs: AppPrefs, systemReduceMotion: Boolean, system24h: Boolean): Appearance = Appearance(
    fontSize = prefs.fontSize,
    is24h = prefs.timeFormat.is24(system24h),
    underlineLinks = prefs.underlineLinks,
    reduceMotion = prefs.motion.reduce(systemReduceMotion),
    jumboEmoji = prefs.jumboEmoji,
)

val LocalAppearance = staticCompositionLocalOf { Appearance() }

/**
 * True when motion should be cut: the user chose it, or the phone has animations off.
 * New animations should read this and skip or shorten themselves.
 */
val LocalReduceMotion = staticCompositionLocalOf { false }

/** True when the phone has animations turned off (developer or accessibility setting). */
fun systemReduceMotion(context: android.content.Context): Boolean = try {
    android.provider.Settings.Global.getFloat(context.contentResolver, android.provider.Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
} catch (_: Exception) {
    false
}
