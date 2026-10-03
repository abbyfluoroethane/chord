package space.foid.chord.ui.sheets

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

private val SAMPLE = listOf(
    "1f600\tgrinning face\tcheerful grin smile\t\n1f604\tgrinning face with smiling eyes\teye happy smile\t",
    "1f436\tdog face\tpet puppy\t",
    "", "", "", "", "", "",
    "2764.fe0f\tred heart\tlove\t",
).joinToString("\n\n")

@RunWith(RobolectricTestRunner::class)
class SheetsLogicTest {
    @Test fun pushRecent_moves_to_front_and_caps() {
        assertEquals(listOf("b", "a"), pushRecent(listOf("a", "b"), "b"))
        assertEquals(listOf("c", "a", "b"), pushRecent(listOf("a", "b"), "c"))
        assertEquals(listOf("c", "a"), pushRecent(listOf("a", "b"), "c", max = 2))
    }

    @Test fun recentEmoji_persists_in_order() {
        val ctx = ApplicationProvider.getApplicationContext<Context>()
        val prefs = ctx.getSharedPreferences("test_recent", Context.MODE_PRIVATE)
        prefs.edit().clear().commit()
        val r = RecentEmoji(prefs)
        assertTrue(r.list().isEmpty())
        r.add("😀")
        r.add("❤️")
        r.add("😀")
        assertEquals(listOf("😀", "❤️"), RecentEmoji(prefs).list())
        for (i in 0 until 40) r.add("x$i")
        assertEquals(RecentEmoji.MAX, r.list().size)
        assertEquals("x39", r.list().first())
    }

    @Test fun decode_reads_groups_and_skips_empty() {
        val groups = decodeEmoji(SAMPLE)
        assertEquals(listOf("smileys", "people", "flags"), groups.map { it.id })
        assertEquals("😀", groups[0].emoji[0].emoji)
        assertEquals("❤️", groups.last().emoji[0].emoji)
    }

    @Test fun decode_keep_drops_emoji() {
        val groups = decodeEmoji(SAMPLE) { it != "😀" }
        assertEquals(1, groups[0].emoji.size)
    }

    @Test fun search_ranks_word_start_first() {
        val groups = decodeEmoji(SAMPLE)
        assertEquals(emptyList<EmojiEntry>(), searchEmoji(groups, "   "))
        // "smil" starts a word in both smileys; "ace" is inside a word only.
        assertEquals(2, searchEmoji(groups, "smil").size)
        assertEquals(3, searchEmoji(groups, "ace").size)
        val r = searchEmoji(groups, "eye")
        assertEquals("😄", r[0].emoji)
        assertEquals("🐶", searchEmoji(groups, "PUPPY")[0].emoji)
        assertEquals(1, searchEmoji(groups, "face", max = 1).size)
        assertTrue(searchEmoji(groups, "zzz").isEmpty())
    }
}
