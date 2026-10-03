package space.foid.chord.ui.sheets

import android.content.Context
import android.content.SharedPreferences

/** The list after one use of [emoji]: it moves to the front, and the list keeps at most [max]. */
fun pushRecent(list: List<String>, emoji: String, max: Int = RecentEmoji.MAX): List<String> =
    (listOf(emoji) + list.filter { it != emoji }).take(max)

/** The emoji the user picked last, newest first. Kept in SharedPreferences. */
class RecentEmoji(private val prefs: SharedPreferences) {
    /** The recent emoji, newest first. */
    fun list(): List<String> =
        prefs.getString(KEY, "").orEmpty().split(SEP).filter { it.isNotEmpty() }

    /** Count one use of [emoji]. */
    fun add(emoji: String) {
        if (emoji.isEmpty()) return
        prefs.edit().putString(KEY, pushRecent(list(), emoji).joinToString(SEP.toString())).apply()
    }

    companion object {
        const val MAX = 24
        private const val KEY = "recent"
        // Emoji never hold a space.
        private const val SEP = ' '

        fun of(context: Context): RecentEmoji =
            RecentEmoji(context.applicationContext.getSharedPreferences("chord_recent_emoji", Context.MODE_PRIVATE))
    }
}
