package space.foid.chord.ui.settings

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.ui.emoji.EmojiPack
import space.foid.chord.ui.theme.MotionMode
import space.foid.chord.ui.theme.TimeFormat
import space.foid.chord.ui.theme.appearanceOf
import space.foid.chord.ui.theme.decodeAccents
import space.foid.chord.ui.theme.encodeAccents
import space.foid.chord.ui.timeline.clockLabel
import space.foid.chord.ui.timeline.stampLabel
import java.time.ZoneId
import java.util.Locale

class AppearancePrefsTest {
    private val memory = object : MemorySettingsStore() {
        override fun persist(prefs: AppPrefs) {}
    }

    @Test fun defaultsKeepTheOldLook() {
        val d = AppPrefs()
        assertEquals("chord-dark", d.darkTheme)
        assertEquals("chord-light", d.lightTheme)
        assertEquals(EmojiPack.System, d.emojiPack)
        assertEquals(15, d.fontSize)
        assertTrue(d.jumboEmoji)
        assertTrue(d.underlineLinks)
    }

    @Test fun fontSizeStaysInRange() {
        memory.update { it.copy(fontSize = 99) }
        assertEquals(20, memory.prefs.value.fontSize)
        memory.update { it.copy(fontSize = 2) }
        assertEquals(12, memory.prefs.value.fontSize)
    }

    @Test fun accentsRoundTrip() {
        val m = mapOf("catppuccin-mocha" to "peach", "x" to "y")
        assertEquals(m, decodeAccents(encodeAccents(m)))
        assertEquals(emptyMap<String, String>(), decodeAccents(""))
        assertEquals(emptyMap<String, String>(), decodeAccents("=a,b="))
    }

    @Test fun timeFormatAndMotionFollowTheSystemOnlyWhenAsked() {
        assertTrue(TimeFormat.System.is24(true))
        assertFalse(TimeFormat.System.is24(false))
        assertFalse(TimeFormat.H12.is24(true))
        assertTrue(TimeFormat.H24.is24(false))
        assertTrue(MotionMode.System.reduce(true))
        assertFalse(MotionMode.System.reduce(false))
        assertTrue(MotionMode.Reduce.reduce(false))
        assertFalse(MotionMode.Full.reduce(true))
        assertEquals(TimeFormat.System, TimeFormat.fromName("nope"))
        assertEquals(MotionMode.Reduce, MotionMode.fromName("Reduce"))
        assertEquals(EmojiPack.Noto, EmojiPack.fromName("Noto"))
        assertEquals(EmojiPack.System, EmojiPack.fromName(null))
    }

    @Test fun appearanceFromPrefs() {
        val a = appearanceOf(AppPrefs(fontSize = 18, timeFormat = TimeFormat.H12, motion = MotionMode.System, jumboEmoji = false), true, true)
        assertEquals(18, a.fontSize)
        assertEquals(1.2f, a.fontFactor, 0.001f)
        assertFalse(a.is24h)
        assertTrue(a.reduceMotion)
        assertFalse(a.jumboEmoji)
    }

    @Test fun clockAndStampIn12And24Hours() {
        val zone = ZoneId.of("UTC")
        val ms = 1_700_000_000_000L + 15 * 3600_000L
        val h24 = clockLabel(ms, zone)
        val h12 = clockLabel(ms, zone, is24 = false)
        assertTrue(h24, Regex("\\d\\d:\\d\\d").matches(h24))
        assertTrue(h12, Regex("\\d{1,2}:\\d\\d (AM|PM)").matches(h12))
        val stamp = stampLabel(ms, ms, zone, Locale.US, is24 = false)
        assertTrue(stamp, stamp.startsWith("today ") && stamp.endsWith("M"))
    }
}
