package space.foid.chord.ui.status

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chord_ffi.Availability

class StatusTextTest {
    @Test fun joinsEmojiAndText() {
        assertEquals("🌴 On holiday", joinStatus("🌴", "On holiday"))
        assertEquals("🌴", joinStatus("🌴", "  "))
        assertEquals("Busy", joinStatus("", " Busy "))
        assertEquals("", joinStatus("", ""))
    }

    @Test fun splitsEmojiAndText() {
        assertEquals(StatusParts("🌴", "On holiday"), splitStatus("🌴 On holiday"))
        assertEquals(StatusParts("🌴", ""), splitStatus("🌴"))
        assertEquals(StatusParts("", "Busy"), splitStatus("Busy"))
        assertEquals(StatusParts("", ""), splitStatus(null))
    }

    @Test fun emojiWithoutSpaceStaysText() {
        assertEquals(StatusParts("", "🌴nice"), splitStatus("🌴nice"))
    }

    @Test fun knowsJoinedFlagAndKeycapEmoji() {
        val family = "👨‍👩‍👧"
        val flag = "🇫🇷"
        val keycap = "1️⃣"
        val heart = "❤️"
        for (e in listOf(family, flag, keycap, heart)) {
            assertEquals(StatusParts(e, "x"), splitStatus("$e x"))
            assertEquals(e, splitStatus(joinStatus(e, "x")).emoji)
        }
        assertEquals(StatusParts("", "1 apple"), splitStatus("1 apple"))
    }

    @Test fun roundTripsAndTrims() {
        val s = joinStatus("😀", "hello")
        assertEquals(StatusParts("😀", "hello"), splitStatus("  $s  "))
    }

    @Test fun lineBreaksBecomeSpaces() {
        assertEquals("a b c d", cleanStatusText("a\nb\r\nc\rd"))
    }

    @Test fun cutsAt128WithoutSplittingAPair() {
        val long = "a".repeat(127) + "😀"
        assertEquals(127, clipStatus(long).length)
        assertEquals(128, clipStatus("b".repeat(300)).length)
        assertEquals(128, statusToSave("", "c".repeat(200))!!.length)
    }

    @Test fun emptyStatusSavesAsNull() {
        assertNull(statusToSave("", "   "))
        assertEquals("😀", statusToSave("😀", ""))
    }

    @Test fun invisibleRowShowsOnlyWhenPossibleOrCurrent() {
        assertFalse(showInvisible(false, Availability.AVAILABLE))
        assertTrue(showInvisible(true, Availability.AVAILABLE))
        assertTrue(showInvisible(false, Availability.INVISIBLE))
        assertEquals(
            listOf(Availability.AVAILABLE, Availability.AWAY, Availability.DND),
            availabilityRows(false, Availability.DND),
        )
        assertEquals(Availability.INVISIBLE, availabilityRows(false, Availability.INVISIBLE).last())
        assertEquals(4, availabilityRows(true, Availability.AWAY).size)
    }
}
