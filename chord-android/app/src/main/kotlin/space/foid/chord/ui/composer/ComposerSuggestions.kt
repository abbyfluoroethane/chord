package space.foid.chord.ui.composer

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import space.foid.chord.R
import space.foid.chord.ui.sheets.EmojiCatalog
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/** One line of the list above the composer. */
@Immutable
sealed interface Suggestion {
    data class Nick(val nick: String) : Suggestion
    data class Emoji(val name: String, val emoji: String) : Suggestion
}

/** The @mention or :emoji: suggestions, above the field. A tap on a row picks it. */
@Composable
fun SuggestionList(items: List<Suggestion>, onPick: (Suggestion) -> Unit, modifier: Modifier = Modifier) {
    val colors = Chord.colors
    Column(modifier.fillMaxWidth().background(colors.surface200)) {
        items.forEach { s ->
            Row(
                Modifier.fillMaxWidth().height(44.dp).clickable(role = Role.Button) { onPick(s) }
                    .padding(horizontal = ChordSpace.s4),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                when (s) {
                    is Suggestion.Nick -> Text(
                        stringResource(R.string.composer_mention_label, s.nick),
                        style = ChordType.body, color = colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    )
                    is Suggestion.Emoji -> {
                        Text(s.emoji, fontSize = 24.sp, modifier = Modifier.width(36.dp))
                        Text(
                            stringResource(R.string.composer_shortcode_label, s.name),
                            style = ChordType.body, color = colors.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

/** The shortcode index of the bundled catalog, built once on a background thread. */
object ShortcodeIndexCache {
    @Volatile private var index: ShortcodeIndex? = null

    /** The index if it is built already, or null. Never waits. */
    fun now(): ShortcodeIndex? = index

    suspend fun load(context: Context): ShortcodeIndex {
        index?.let { return it }
        val groups = EmojiCatalog.load(context)
        return withContext(Dispatchers.Default) {
            index ?: run {
                val known = runCatching {
                    parseShortcodes(context.applicationContext.assets.open("shortcodes.txt").bufferedReader().use { it.readText() })
                }.getOrDefault(emptyList())
                ShortcodeIndex(groups, known).also { index = it }
            }
        }
    }
}
