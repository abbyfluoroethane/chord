package space.foid.chord.ui.sheets

import android.content.Context
import android.graphics.Paint
import androidx.compose.runtime.Immutable
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** One emoji of the picker. [words] is the label and the tags in lower case, for the search. */
@Immutable
data class EmojiEntry(val emoji: String, val label: String, val words: String)

/** One category of the picker. */
@Immutable
data class EmojiGroup(val id: String, val label: String, val emoji: List<EmojiEntry>)

/** The groups in file order, as in chord-desktop emojibuild.ts. */
private val GROUP_LABELS = listOf(
    "smileys" to "Smileys",
    "people" to "People",
    "nature" to "Nature",
    "food" to "Food",
    "travel" to "Travel",
    "activities" to "Activities",
    "objects" to "Objects",
    "symbols" to "Symbols",
    "flags" to "Flags",
)

private fun fromHex(hex: String): String {
    val sb = StringBuilder()
    for (cp in hex.split('.')) sb.appendCodePoint(cp.toInt(16))
    return sb.toString()
}

private val WIDE = Regex("\\\\u([0-9a-f]{4})")

private fun unescapeWide(s: String): String =
    if ('\\' in s) WIDE.replace(s) { it.groupValues[1].toInt(16).toChar().toString() } else s

/**
 * Reads assets/emoji.txt (the file of the desktop picker): one line for each emoji with the
 * fields hex, label, tags, skins separated by a tab; an empty line between two groups.
 * [keep] drops emoji that the system font cannot draw.
 */
fun decodeEmoji(text: String, keep: (String) -> Boolean = { true }): List<EmojiGroup> {
    val blocks = text.split("\n\n")
    return GROUP_LABELS.mapIndexed { n, (id, label) ->
        val list = (blocks.getOrNull(n) ?: "").lineSequence().filter { it.isNotEmpty() }.mapNotNull { line ->
            val f = line.split('\t')
            val emoji = fromHex(f[0])
            if (!keep(emoji)) return@mapNotNull null
            val lab = unescapeWide(f.getOrElse(1) { "" })
            val tags = unescapeWide(f.getOrElse(2) { "" })
            EmojiEntry(emoji, lab, "${lab.lowercase()} $tags")
        }.toList()
        EmojiGroup(id, label, list)
    }.filter { it.emoji.isNotEmpty() }
}

/** The emoji that match [text], best first: a word that starts with the text first. */
fun searchEmoji(groups: List<EmojiGroup>, text: String, max: Int = 80): List<EmojiEntry> {
    val q = text.trim().lowercase()
    if (q.isEmpty()) return emptyList()
    val starts = ArrayList<EmojiEntry>()
    val contains = ArrayList<EmojiEntry>()
    for (g in groups) for (e in g.emoji) {
        val at = e.words.indexOf(q)
        if (at < 0) continue
        if (at == 0 || e.words[at - 1] == ' ') {
            starts += e
            if (starts.size >= max) return starts
        } else {
            contains += e
        }
    }
    return (starts + contains).take(max)
}

/** The emoji data, loaded once. */
object EmojiCatalog {
    @Volatile private var groups: List<EmojiGroup>? = null

    /** The groups if they are loaded already, or null. Never waits. */
    fun now(): List<EmojiGroup>? = groups

    /**
     * Loads the data on a background thread; later calls return the same list. Call it early
     * (for example at the start of the app) and the picker opens with no wait.
     */
    suspend fun load(context: Context): List<EmojiGroup> {
        groups?.let { return it }
        return withContext(Dispatchers.Default) {
            groups ?: run {
                val text = context.applicationContext.assets.open("emoji.txt").bufferedReader().use { it.readText() }
                val paint = Paint()
                decodeEmoji(text) { paint.hasGlyph(it) }.also { groups = it }
            }
        }
    }
}
