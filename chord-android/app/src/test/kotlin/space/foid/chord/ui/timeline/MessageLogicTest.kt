package space.foid.chord.ui.timeline

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chord_ffi.DeliveryStatus
import uniffi.chord_ffi.ReactionSummary
import uniffi.chord_ffi.ReplyPreview
import uniffi.chord_ffi.TimelineItem
import java.time.ZoneId
import java.time.ZonedDateTime

class MessageLogicTest {
    private val utc = ZoneId.of("UTC")
    private fun at(h: Int, m: Int, day: Int = 1) =
        ZonedDateTime.of(2026, 3, day, h, m, 0, 0, utc).toInstant().toEpochMilli()

    private fun ui(
        sender: String = "a@x",
        ts: Long = at(10, 0),
        same: Boolean = true,
        reply: ReplyUi? = null,
    ) = MessageUi(
        id = "m:1", senderId = sender, senderName = sender, avatarUrl = null, body = "hi", timestamp = ts,
        timeLabel = "", outgoing = false, sameSenderAsPrevious = same, edited = false, retracted = false, reply = reply,
    )

    @Test fun first_row_is_a_head() = assertFalse(continuesGroup(null, ui(), zone = utc))

    @Test fun close_rows_of_one_sender_group() =
        assertTrue(continuesGroup(ui(ts = at(10, 0)), ui(ts = at(10, 4)), zone = utc))

    @Test fun gap_of_five_minutes_breaks() =
        assertFalse(continuesGroup(ui(ts = at(10, 0)), ui(ts = at(10, 5)), zone = utc))

    @Test fun other_sender_breaks() =
        assertFalse(continuesGroup(ui(sender = "a@x"), ui(sender = "b@x", ts = at(10, 1)), zone = utc))

    @Test fun core_flag_false_breaks() =
        assertFalse(continuesGroup(ui(), ui(ts = at(10, 1), same = false), zone = utc))

    @Test fun reply_breaks() =
        assertFalse(continuesGroup(ui(), ui(ts = at(10, 1), reply = ReplyUi("m:0", "b", "yo")), zone = utc))

    @Test fun divider_breaks() =
        assertFalse(continuesGroup(ui(), ui(ts = at(10, 1)), dividerBefore = true, zone = utc))

    @Test fun new_day_breaks() =
        assertFalse(continuesGroup(ui(ts = at(23, 59)), ui(ts = at(0, 1, day = 2)), zone = utc))

    @Test fun time_going_back_breaks() =
        assertFalse(continuesGroup(ui(ts = at(10, 5)), ui(ts = at(10, 1)), zone = utc))

    private fun item(
        status: DeliveryStatus = DeliveryStatus.SENT,
        replyTo: ReplyPreview? = null,
        reactions: List<ReactionSummary> = emptyList(),
    ) = TimelineItem(
        id = "m:7", stanzaId = null, originId = null, sender = "alice@x.org", senderName = "Alice", avatar = "u",
        body = "hello", timestamp = at(14, 5), outgoing = true, sameSenderAsPrevious = true, edited = true,
        retracted = false, reactions = reactions, replyTo = replyTo, attachment = null, status = status,
    )

    @Test fun maps_plain_fields() {
        val m = item().toMessageUi(zone = utc)
        assertEquals("m:7", m.id)
        assertEquals("alice@x.org", m.senderId)
        assertEquals("14:05", m.timeLabel)
        assertTrue(m.edited && m.outgoing && m.sameSenderAsPrevious)
        assertEquals(SendState.SENT, m.state)
        assertNull(m.reply)
    }

    @Test fun maps_reactions_and_reply() {
        val m = item(
            replyTo = ReplyPreview("m:3", "Bob", "\n  first line \nsecond"),
            reactions = listOf(ReactionSummary("👍", 3u, true)),
        ).toMessageUi(zone = utc)
        assertEquals(ReplyUi("m:3", "Bob", "first line"), m.reply)
        assertEquals(listOf(ReactionUi("👍", 3, true)), m.reactions)
    }

    @Test fun maps_states() {
        assertEquals(SendState.FAILED, item(DeliveryStatus.FAILED).toMessageUi(pending = true, zone = utc).state)
        assertEquals(SendState.PENDING, item().toMessageUi(pending = true, zone = utc).state)
    }

    @Test fun snippet_is_cut() {
        val s = replySnippet("x".repeat(500), max = 10)
        assertEquals(11, s.length)
        assertTrue(s.endsWith("…"))
    }
}
