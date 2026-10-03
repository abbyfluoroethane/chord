package space.foid.chord.ui.status

import uniffi.chord_ffi.Availability

/** The longest status text the core keeps. */
const val STATUS_MAX = 128

/** A saved status as its emoji and its text. */
data class StatusParts(val emoji: String, val text: String)

private fun isPictographic(cp: Int): Boolean =
    cp in 0x1F000..0x1FAFF || cp in 0x2190..0x21FF || cp in 0x2300..0x23FF || cp in 0x2460..0x24FF ||
        cp in 0x25A0..0x27BF || cp in 0x2900..0x297F || cp in 0x2B00..0x2BFF ||
        cp == 0xA9 || cp == 0xAE || cp == 0x203C || cp == 0x2049 || cp == 0x2122 || cp == 0x2139 ||
        cp == 0x3030 || cp == 0x303D || cp == 0x3297 || cp == 0x3299

private fun isRegional(cp: Int) = cp in 0x1F1E6..0x1F1FF
private fun isModifier(cp: Int) = cp in 0x1F3FB..0x1F3FF

/**
 * The length in chars of the emoji at the start of [s], or 0. It knows flags, keycaps, skin
 * tones and joined sequences, which is close to the desktop's RGI match.
 */
fun leadingEmojiLength(s: String): Int {
    if (s.isEmpty()) return 0
    fun cpAt(k: Int) = if (k < s.length) s.codePointAt(k) else -1
    val first = cpAt(0)
    var i = Character.charCount(first)
    if (isRegional(first)) {
        val second = cpAt(i)
        return if (isRegional(second)) i + Character.charCount(second) else 0
    }
    if (first == '#'.code || first == '*'.code || first in '0'.code..'9'.code) {
        if (cpAt(i) == 0xFE0F) i++
        return if (cpAt(i) == 0x20E3) i + 1 else 0
    }
    if (!isPictographic(first)) return 0
    while (true) {
        if (cpAt(i) == 0xFE0F) i++
        if (isModifier(cpAt(i))) i += Character.charCount(cpAt(i))
        if (cpAt(i) == 0x200D && isPictographic(cpAt(i + 1))) {
            i += 1 + Character.charCount(cpAt(i + 1))
        } else {
            return i
        }
    }
}

/**
 * Split a saved status. A status that starts with one emoji and a space, or is only one emoji,
 * gives that emoji. The same rule as the desktop (emojisplit.ts).
 */
fun splitStatus(status: String?): StatusParts {
    val s = status?.trim().orEmpty()
    val n = leadingEmojiLength(s)
    if (n > 0 && (n == s.length || s[n] == ' ')) return StatusParts(s.substring(0, n), s.substring(n).trim())
    return StatusParts("", s)
}

/** The status to save: the emoji, a space, then the text. Either part can be empty. */
fun joinStatus(emoji: String, text: String): String {
    val t = text.trim()
    return if (emoji.isNotEmpty()) (if (t.isNotEmpty()) "$emoji $t" else emoji) else t
}

/** Line breaks become spaces, and the text is cut at [STATUS_MAX] chars. */
fun cleanStatusText(raw: String): String =
    clipStatus(raw.replace("\r\n", " ").replace('\n', ' ').replace('\r', ' '))

/** Cut at [STATUS_MAX] chars without splitting a surrogate pair. */
fun clipStatus(s: String): String {
    if (s.length <= STATUS_MAX) return s
    val end = if (Character.isHighSurrogate(s[STATUS_MAX - 1])) STATUS_MAX - 1 else STATUS_MAX
    return s.substring(0, end)
}

/** The value to save for a status: trimmed, cut, or null when empty. */
fun statusToSave(emoji: String, text: String): String? =
    clipStatus(joinStatus(emoji, text).trim()).ifEmpty { null }

/**
 * The availabilities on offer, in the order of the desktop menu. "Invisible" shows only when
 * the server can hide us, or when it is the current value.
 */
fun availabilityRows(canHide: Boolean, current: Availability): List<Availability> = buildList {
    add(Availability.AVAILABLE)
    add(Availability.AWAY)
    add(Availability.DND)
    if (showInvisible(canHide, current)) add(Availability.INVISIBLE)
}

fun showInvisible(canHide: Boolean, current: Availability): Boolean =
    canHide || current == Availability.INVISIBLE
